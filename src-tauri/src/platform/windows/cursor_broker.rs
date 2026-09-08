use std::sync::mpsc;
use std::thread::{self, JoinHandle};

use windows::Win32::Foundation::{HWND, LPARAM, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::ClientToScreen;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
use windows::Win32::UI::WindowsAndMessaging::{
    ClipCursor, DispatchMessageW, GetClientRect, GetClipCursor, GetForegroundWindow, GetMessageW,
    GetWindowThreadProcessId, PeekMessageW, PostThreadMessageW, SetCursorPos, TranslateMessage,
    EVENT_OBJECT_LOCATIONCHANGE, EVENT_SYSTEM_FOREGROUND, MSG, PM_NOREMOVE, WINEVENT_OUTOFCONTEXT,
    WM_APP,
};

const PROTOCOL_PREFIX: &str = "MONALAUNCHER_CURSOR\t";
const WM_CURSOR_GRAB: u32 = WM_APP + 1;
const WM_CURSOR_RELEASE: u32 = WM_APP + 2;
const WM_CURSOR_WINDOW_EVENT: u32 = WM_APP + 3;
const WM_CURSOR_STOP: u32 = WM_APP + 4;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CursorCommand {
    Grab,
    Release,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProtocolLine {
    NotProtocol,
    Consumed(Option<CursorCommand>),
}

pub struct CursorBroker {
    expected_token: String,
    thread_id: u32,
    thread: Option<JoinHandle<()>>,
}

impl CursorBroker {
    pub fn start(process_id: u32, expected_token: String) -> Result<Self, String> {
        if expected_token.len() != 64
            || !expected_token.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err("cursor broker token has an invalid format".to_owned());
        }

        let (startup_tx, startup_rx) = mpsc::sync_channel(1);
        let thread = thread::spawn(move || run(process_id, startup_tx));
        let thread_id = match startup_rx.recv() {
            Ok(Ok(thread_id)) => thread_id,
            Ok(Err(error)) => {
                let _ = thread.join();
                return Err(error);
            }
            Err(_) => {
                let _ = thread.join();
                return Err("cursor broker stopped during startup".to_owned());
            }
        };

        Ok(Self {
            expected_token,
            thread_id,
            thread: Some(thread),
        })
    }

    /// Consumes authenticated cursor-state lines emitted from the instrumented GLFW API.
    pub fn handle_line(&self, line: &str) -> bool {
        match parse_protocol_line(line, &self.expected_token) {
            ProtocolLine::NotProtocol => false,
            ProtocolLine::Consumed(command) => {
                let message = match command {
                    Some(CursorCommand::Grab) => Some(WM_CURSOR_GRAB),
                    Some(CursorCommand::Release) => Some(WM_CURSOR_RELEASE),
                    None => None,
                };
                if let Some(message) = message {
                    // SAFETY: the broker creates its message queue before publishing thread_id.
                    let _ = unsafe {
                        PostThreadMessageW(self.thread_id, message, WPARAM(0), LPARAM(0))
                    };
                }
                true
            }
        }
    }
}

impl Drop for CursorBroker {
    fn drop(&mut self) {
        // SAFETY: the message only asks this broker's private thread to stop.
        let _ = unsafe { PostThreadMessageW(self.thread_id, WM_CURSOR_STOP, WPARAM(0), LPARAM(0)) };
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn parse_protocol_line(line: &str, expected_token: &str) -> ProtocolLine {
    let Some(marker) = line.find(PROTOCOL_PREFIX) else {
        return ProtocolLine::NotProtocol;
    };
    let payload = &line[marker + PROTOCOL_PREFIX.len()..];
    let Some((token, command)) = payload.split_once('\t') else {
        return ProtocolLine::Consumed(None);
    };
    if token != expected_token {
        return ProtocolLine::Consumed(None);
    }
    ProtocolLine::Consumed(match command {
        "GRAB" => Some(CursorCommand::Grab),
        "RELEASE" => Some(CursorCommand::Release),
        _ => None,
    })
}

fn run(process_id: u32, startup: mpsc::SyncSender<Result<u32, String>>) {
    // SAFETY: this creates the message queue before other threads may call PostThreadMessageW.
    unsafe {
        let mut bootstrap_message = MSG::default();
        let _ = PeekMessageW(&mut bootstrap_message, None, 0, 0, PM_NOREMOVE);
    }

    let hooks = match install_event_hooks(process_id) {
        Ok(hooks) => hooks,
        Err(error) => {
            let _ = startup.send(Err(error));
            return;
        }
    };
    // SAFETY: returns the identifier of the current broker thread.
    let thread_id = unsafe { GetCurrentThreadId() };
    if startup.send(Ok(thread_id)).is_err() {
        uninstall_event_hooks(hooks);
        return;
    }

    let mut requested_grab = false;
    let mut owned_clip = None;
    loop {
        let mut message = MSG::default();
        // SAFETY: message points to initialized writable storage. This sleeps until either a
        // Minecraft cursor command or a subscribed Windows event reaches the queue.
        let result = unsafe { GetMessageW(&mut message, None, 0, 0) }.0;
        if result <= 0 {
            break;
        }
        match message.message {
            WM_CURSOR_GRAB => {
                requested_grab = true;
                apply_cursor_state(process_id, true, false, &mut owned_clip);
            }
            WM_CURSOR_RELEASE => {
                requested_grab = false;
                apply_cursor_state(process_id, false, true, &mut owned_clip);
            }
            WM_CURSOR_WINDOW_EVENT => {
                apply_cursor_state(process_id, requested_grab, false, &mut owned_clip);
            }
            WM_CURSOR_STOP => break,
            // SAFETY: standard message-loop forwarding for messages not owned by this broker.
            _ => unsafe {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            },
        }
    }

    release_owned_clip(&mut owned_clip);
    uninstall_event_hooks(hooks);
}

fn install_event_hooks(process_id: u32) -> Result<[HWINEVENTHOOK; 2], String> {
    // SAFETY: out-of-context hooks keep the callback in this trusted process. The broker thread
    // owns a message loop, and both hooks are removed from that same thread before it exits.
    unsafe {
        let foreground = SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            None,
            Some(win_event_callback),
            0,
            0,
            WINEVENT_OUTOFCONTEXT,
        );
        if foreground.is_invalid() {
            return Err("failed to subscribe to foreground-window events".to_owned());
        }
        let location = SetWinEventHook(
            EVENT_OBJECT_LOCATIONCHANGE,
            EVENT_OBJECT_LOCATIONCHANGE,
            None,
            Some(win_event_callback),
            process_id,
            0,
            WINEVENT_OUTOFCONTEXT,
        );
        if location.is_invalid() {
            let _ = UnhookWinEvent(foreground);
            return Err("failed to subscribe to Minecraft window-location events".to_owned());
        }
        Ok([foreground, location])
    }
}

fn uninstall_event_hooks(hooks: [HWINEVENTHOOK; 2]) {
    // SAFETY: called by the same broker thread that installed these valid hook handles.
    unsafe {
        for hook in hooks {
            let _ = UnhookWinEvent(hook);
        }
    }
}

unsafe extern "system" fn win_event_callback(
    _hook: HWINEVENTHOOK,
    event: u32,
    _window: HWND,
    object_id: i32,
    _child_id: i32,
    _event_thread: u32,
    _event_time: u32,
) {
    // EVENT_OBJECT_LOCATIONCHANGE also covers accessibility child objects. Minecraft's top-level
    // client rectangle changes are reported with OBJID_WINDOW (zero).
    if event == EVENT_OBJECT_LOCATIONCHANGE && object_id != 0 {
        return;
    }
    // Out-of-context WinEvents are delivered on the hook-owning thread. Queueing a private message
    // keeps cursor mutation outside the callback and avoids callback reentrancy.
    // SAFETY: this callback runs on the hook-owning thread, whose queue remains alive until both
    // hooks are removed.
    let _ = unsafe {
        PostThreadMessageW(
            GetCurrentThreadId(),
            WM_CURSOR_WINDOW_EVENT,
            WPARAM(0),
            LPARAM(0),
        )
    };
}

fn apply_cursor_state(
    process_id: u32,
    requested_grab: bool,
    center_on_release: bool,
    owned_clip: &mut Option<RECT>,
) {
    let game_clip = foreground_game_clip(process_id);
    if center_on_release {
        release_owned_clip(owned_clip);
        if let Some(rect) = game_clip {
            let (x, y) = rect_center(rect);
            // SAFETY: the coordinates are inside the foreground Minecraft client area.
            let _ = unsafe { SetCursorPos(x, y) };
        }
        return;
    }

    match requested_grab.then_some(game_clip).flatten() {
        Some(rect) if *owned_clip != Some(rect) => {
            release_owned_clip(owned_clip);
            // SAFETY: rect remains valid for the duration of the call. This trusted parent process
            // performs the shared-cursor operation denied to the AppContainer child.
            if unsafe { ClipCursor(Some(&rect)) }.is_ok() {
                let (x, y) = rect_center(rect);
                // SAFETY: the target coordinates are inside the foreground game window.
                let _ = unsafe { SetCursorPos(x, y) };
                *owned_clip = Some(rect);
            }
        }
        None => release_owned_clip(owned_clip),
        Some(_) => {}
    }
}

fn foreground_game_clip(process_id: u32) -> Option<RECT> {
    // SAFETY: these calls only inspect the current foreground window and its client rectangle.
    unsafe {
        let window = GetForegroundWindow();
        let mut foreground_process_id = 0;
        GetWindowThreadProcessId(window, Some(&mut foreground_process_id));
        if foreground_process_id != process_id {
            return None;
        }

        let mut client = RECT::default();
        GetClientRect(window, &mut client).ok()?;
        let mut top_left = POINT {
            x: client.left,
            y: client.top,
        };
        let mut bottom_right = POINT {
            x: client.right,
            y: client.bottom,
        };
        if !ClientToScreen(window, &mut top_left).as_bool()
            || !ClientToScreen(window, &mut bottom_right).as_bool()
        {
            return None;
        }
        let rect = RECT {
            left: top_left.x,
            top: top_left.y,
            right: bottom_right.x,
            bottom: bottom_right.y,
        };
        (rect.right > rect.left && rect.bottom > rect.top).then_some(rect)
    }
}

fn release_owned_clip(owned_clip: &mut Option<RECT>) {
    let Some(expected) = owned_clip.take() else {
        return;
    };
    let mut current = RECT::default();

    // Only release the rectangle this broker installed. This avoids clearing a clip installed by
    // another foreground application during a focus transition.
    // SAFETY: current points to writable memory and None is the documented way to release a clip.
    unsafe {
        if GetClipCursor(&mut current).is_ok() && current == expected {
            let _ = ClipCursor(None);
        }
    }
}

fn rect_center(rect: RECT) -> (i32, i32) {
    // Calculate in i64 so a hostile or malformed window rectangle cannot overflow the broker
    // thread and leave a previously installed cursor clip unreleased.
    (
        ((i64::from(rect.left) + i64::from(rect.right)) / 2) as i32,
        ((i64::from(rect.top) + i64::from(rect.bottom)) / 2) as i32,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_window_rectangle_center() {
        assert_eq!(
            rect_center(RECT {
                left: 100,
                top: 50,
                right: 900,
                bottom: 650,
            }),
            (500, 350)
        );
        assert_eq!(
            rect_center(RECT {
                left: i32::MIN,
                top: i32::MIN,
                right: i32::MAX,
                bottom: i32::MAX,
            }),
            (0, 0)
        );
    }

    #[test]
    fn accepts_only_authenticated_minecraft_cursor_signals() {
        let token = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

        assert_eq!(
            parse_protocol_line("ordinary Minecraft log", token),
            ProtocolLine::NotProtocol
        );
        assert_eq!(
            parse_protocol_line(
                &format!("[Render thread/INFO]: [STDOUT]: MONALAUNCHER_CURSOR\t{token}\tGRAB"),
                token
            ),
            ProtocolLine::Consumed(Some(CursorCommand::Grab))
        );
        assert_eq!(
            parse_protocol_line("MONALAUNCHER_CURSOR\twrong\tRELEASE", token),
            ProtocolLine::Consumed(None)
        );
        assert_eq!(
            parse_protocol_line(&format!("MONALAUNCHER_CURSOR\t{token}\tRELEASE"), token),
            ProtocolLine::Consumed(Some(CursorCommand::Release))
        );
    }

    #[test]
    fn starts_and_stops_the_event_driven_broker() {
        let token = "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";
        let broker = CursorBroker::start(std::process::id(), token.to_owned())
            .expect("WinEvent hooks should initialize");
        assert!(broker.handle_line(&format!("MONALAUNCHER_CURSOR\t{token}\tRELEASE")));
        drop(broker);
    }
}
