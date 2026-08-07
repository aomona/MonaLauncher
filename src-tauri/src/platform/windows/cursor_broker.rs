use std::mem::size_of;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use windows::Win32::Foundation::{POINT, RECT};
use windows::Win32::Graphics::Gdi::ClientToScreen;
use windows::Win32::UI::WindowsAndMessaging::{
    ClipCursor, GetClientRect, GetClipCursor, GetCursorInfo, GetForegroundWindow,
    GetWindowThreadProcessId, SetCursorPos, CURSORINFO, CURSOR_SHOWING,
};

const POLL_INTERVAL: Duration = Duration::from_millis(8);

pub struct CursorBroker {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl CursorBroker {
    pub fn start(process_id: u32) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        let thread = thread::spawn(move || run(process_id, &thread_stop));

        Self {
            stop,
            thread: Some(thread),
        }
    }
}

impl Drop for CursorBroker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn run(process_id: u32, stop: &AtomicBool) {
    let mut owned_clip = None;

    while !stop.load(Ordering::Acquire) {
        let requested_clip = foreground_game_clip(process_id);
        match requested_clip {
            Some(rect) if owned_clip != Some(rect) => {
                release_owned_clip(&mut owned_clip);
                // SAFETY: rect remains valid for the duration of the call. This trusted parent
                // process performs the shared-cursor operation denied to the AppContainer child.
                if unsafe { ClipCursor(Some(&rect)) }.is_ok() {
                    let (x, y) = rect_center(rect);
                    // SAFETY: the target coordinates are inside the foreground game window.
                    let _ = unsafe { SetCursorPos(x, y) };
                    owned_clip = Some(rect);
                }
            }
            None => release_owned_clip(&mut owned_clip),
            Some(_) => {}
        }

        thread::sleep(POLL_INTERVAL);
    }

    release_owned_clip(&mut owned_clip);
}

fn foreground_game_clip(process_id: u32) -> Option<RECT> {
    // SAFETY: these calls only inspect the current foreground window and cursor state.
    unsafe {
        let window = GetForegroundWindow();
        let mut foreground_process_id = 0;
        GetWindowThreadProcessId(window, Some(&mut foreground_process_id));
        if foreground_process_id != process_id {
            return None;
        }

        let mut cursor = CURSORINFO {
            cbSize: size_of::<CURSORINFO>() as u32,
            ..Default::default()
        };
        GetCursorInfo(&mut cursor).ok()?;
        if cursor.flags.0 & CURSOR_SHOWING.0 != 0 {
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
    (
        rect.left + (rect.right - rect.left) / 2,
        rect.top + (rect.bottom - rect.top) / 2,
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
    }
}
