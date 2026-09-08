use std::sync::mpsc::{self, SyncSender};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use windows::core::PCWSTR;
use windows::Win32::Media::Speech::{
    ISpVoice, SpVoice, SPF_ASYNC, SPF_IS_NOT_XML, SPF_PURGEBEFORESPEAK,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
};

const PROTOCOL_PREFIX: &str = "MONALAUNCHER_NARRATOR\t";
const MAX_TEXT_BYTES: usize = 4 * 1024;
const MAX_ENCODED_BYTES: usize = MAX_TEXT_BYTES.div_ceil(3) * 4;
const QUEUE_CAPACITY: usize = 8;
const MAX_COMMANDS_PER_SECOND: u32 = 8;
const DUPLICATE_WINDOW: Duration = Duration::from_millis(100);
const INITIALIZATION_TIMEOUT: Duration = Duration::from_secs(5);

enum NarratorCommand {
    Say {
        text: String,
        interrupt: bool,
        volume: f32,
    },
    Clear,
}

pub struct NarratorBroker {
    expected_token: String,
    sender: SyncSender<NarratorCommand>,
    limits: Mutex<BrokerLimits>,
}

struct BrokerLimits {
    window_started: Instant,
    accepted: u32,
    last_text: Option<(String, Instant)>,
}

impl NarratorBroker {
    pub fn start(expected_token: String) -> Result<Self, String> {
        if expected_token.len() != 64
            || !expected_token.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err("narrator token has an invalid format".to_owned());
        }
        let (sender, receiver) = mpsc::sync_channel(QUEUE_CAPACITY);
        let (ready_sender, ready_receiver) = mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let initialized = initialize_voice();
            let _ = ready_sender.send(
                initialized
                    .as_ref()
                    .map(|_| ())
                    .map_err(ToString::to_string),
            );
            let Ok(voice) = initialized else {
                return;
            };

            while let Ok(command) = receiver.recv() {
                match command {
                    NarratorCommand::Say {
                        text,
                        interrupt,
                        volume,
                    } => speak(&voice, &text, interrupt, volume),
                    NarratorCommand::Clear => clear(&voice),
                }
            }

            drop(voice);
            // SAFETY: this thread successfully initialized COM and owns the voice until here.
            unsafe { CoUninitialize() };
        });

        ready_receiver
            .recv_timeout(INITIALIZATION_TIMEOUT)
            .map_err(|_| "narrator broker initialization timed out".to_owned())??;
        Ok(Self {
            expected_token,
            sender,
            limits: Mutex::new(BrokerLimits {
                window_started: Instant::now(),
                accepted: 0,
                last_text: None,
            }),
        })
    }

    /// Consumes launcher narrator protocol lines and returns true when the line is protocol data.
    pub fn handle_line(&self, line: &str) -> bool {
        let Some(marker) = line.find(PROTOCOL_PREFIX) else {
            return false;
        };
        let payload = &line[marker + PROTOCOL_PREFIX.len()..];
        let Some((token, command)) = payload.split_once('\t') else {
            return true;
        };
        if token != self.expected_token {
            return true;
        }
        if command == "CLEAR" {
            if self.allow_command(None) {
                let _ = self.sender.try_send(NarratorCommand::Clear);
            }
            return true;
        }

        let mut fields = command.splitn(4, '\t');
        if fields.next() != Some("SAY") {
            return true;
        }
        let interrupt = fields.next() == Some("1");
        let volume = fields
            .next()
            .and_then(|value| value.parse::<f32>().ok())
            .unwrap_or(1.0)
            .clamp(0.0, 1.0);
        let Some(encoded) = fields.next() else {
            return true;
        };
        if encoded.len() > MAX_ENCODED_BYTES {
            return true;
        }
        let Ok(bytes) = STANDARD.decode(encoded) else {
            return true;
        };
        if bytes.len() > MAX_TEXT_BYTES {
            return true;
        }
        let Ok(text) = String::from_utf8(bytes) else {
            return true;
        };
        if self.allow_command(Some(&text)) {
            let _ = self.sender.try_send(NarratorCommand::Say {
                text,
                interrupt,
                volume,
            });
        }
        true
    }

    fn allow_command(&self, text: Option<&str>) -> bool {
        let now = Instant::now();
        let Ok(mut limits) = self.limits.lock() else {
            return false;
        };
        if now.duration_since(limits.window_started) >= Duration::from_secs(1) {
            limits.window_started = now;
            limits.accepted = 0;
        }
        if limits.accepted >= MAX_COMMANDS_PER_SECOND {
            return false;
        }
        if let (Some(text), Some((last_text, last_at))) = (text, &limits.last_text) {
            if text == last_text && now.duration_since(*last_at) < DUPLICATE_WINDOW {
                return false;
            }
        }
        limits.accepted += 1;
        if let Some(text) = text {
            limits.last_text = Some((text.to_owned(), now));
        }
        true
    }
}

fn initialize_voice() -> windows::core::Result<ISpVoice> {
    // SAFETY: the broker thread balances this call with CoUninitialize before exiting.
    unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()? };
    // SAFETY: SpVoice is an in-process/out-of-process COM class requested on the initialized thread.
    match unsafe { CoCreateInstance(&SpVoice, None, CLSCTX_ALL) } {
        Ok(voice) => Ok(voice),
        Err(error) => {
            // SAFETY: COM was initialized successfully above, but no voice will own this apartment.
            unsafe { CoUninitialize() };
            Err(error)
        }
    }
}

fn speak(voice: &ISpVoice, text: &str, interrupt: bool, volume: f32) {
    let text = text.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
    let mut flags = (SPF_ASYNC.0 | SPF_IS_NOT_XML.0) as u32;
    if interrupt {
        flags |= SPF_PURGEBEFORESPEAK.0 as u32;
    }
    // SAFETY: text is NUL-terminated and remains alive for the duration of both COM calls.
    unsafe {
        let _ = voice.SetVolume((volume * 100.0).round() as u16);
        let _ = voice.Speak(PCWSTR(text.as_ptr()), flags, None);
    }
}

fn clear(voice: &ISpVoice) {
    let empty = [0_u16];
    let flags = (SPF_ASYNC.0 | SPF_PURGEBEFORESPEAK.0 | SPF_IS_NOT_XML.0) as u32;
    // SAFETY: empty is a valid NUL-terminated UTF-16 string for the duration of the COM call.
    unsafe {
        let _ = voice.Speak(PCWSTR(empty.as_ptr()), flags, None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    fn test_broker() -> (NarratorBroker, mpsc::Receiver<NarratorCommand>) {
        let (sender, receiver) = mpsc::sync_channel(QUEUE_CAPACITY);
        (
            NarratorBroker {
                expected_token: TOKEN.to_owned(),
                sender,
                limits: Mutex::new(BrokerLimits {
                    window_started: Instant::now(),
                    accepted: 0,
                    last_text: None,
                }),
            },
            receiver,
        )
    }

    #[test]
    fn ignores_regular_log_lines() {
        let (broker, _receiver) = test_broker();

        assert!(!broker.handle_line("ordinary Minecraft log"));
    }

    #[test]
    fn accepts_protocol_wrapped_by_log4j() {
        let (broker, receiver) = test_broker();

        assert!(broker.handle_line(&format!(
            "[Render thread/INFO]: [STDOUT]: MONALAUNCHER_NARRATOR\t{TOKEN}\tCLEAR"
        )));
        assert!(matches!(receiver.recv().unwrap(), NarratorCommand::Clear));
    }

    #[test]
    fn rejects_protocol_with_the_wrong_token() {
        let (broker, receiver) = test_broker();

        assert!(broker.handle_line("MONALAUNCHER_NARRATOR\twrong\tCLEAR"));
        assert!(receiver.try_recv().is_err());
    }

    #[test]
    fn suppresses_immediate_duplicate_speech() {
        let (broker, receiver) = test_broker();
        let line = format!("MONALAUNCHER_NARRATOR\t{TOKEN}\tSAY\t1\t1.0\taGVsbG8=");

        assert!(broker.handle_line(&line));
        assert!(broker.handle_line(&line));
        assert!(matches!(
            receiver.recv().unwrap(),
            NarratorCommand::Say { .. }
        ));
        assert!(receiver.try_recv().is_err());
    }

    #[test]
    fn bounds_the_command_queue_and_rate() {
        let (broker, receiver) = test_broker();
        let line = format!("MONALAUNCHER_NARRATOR\t{TOKEN}\tCLEAR");

        for _ in 0..64 {
            assert!(broker.handle_line(&line));
        }

        assert_eq!(receiver.try_iter().count(), QUEUE_CAPACITY);
    }

    #[test]
    fn rejects_oversized_base64_before_decoding() {
        let (broker, receiver) = test_broker();
        let encoded = "A".repeat(MAX_ENCODED_BYTES + 1);
        let line = format!("MONALAUNCHER_NARRATOR\t{TOKEN}\tSAY\t1\t1.0\t{encoded}");

        assert!(broker.handle_line(&line));
        assert!(receiver.try_recv().is_err());
    }
}
