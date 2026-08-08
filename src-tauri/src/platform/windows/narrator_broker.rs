use std::sync::mpsc::{self, Sender};

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
const MAX_TEXT_BYTES: usize = 32 * 1024;

enum NarratorCommand {
    Say {
        text: String,
        interrupt: bool,
        volume: f32,
    },
    Clear,
}

pub struct NarratorBroker {
    sender: Sender<NarratorCommand>,
}

impl NarratorBroker {
    pub fn start() -> Result<Self, String> {
        let (sender, receiver) = mpsc::channel();
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
            .recv()
            .map_err(|_| "narrator broker stopped during initialization".to_owned())??;
        Ok(Self { sender })
    }

    /// Consumes launcher narrator protocol lines and returns true when the line is protocol data.
    pub fn handle_line(&self, line: &str) -> bool {
        let Some(marker) = line.find(PROTOCOL_PREFIX) else {
            return false;
        };
        let payload = &line[marker + PROTOCOL_PREFIX.len()..];
        if payload == "CLEAR" {
            let _ = self.sender.send(NarratorCommand::Clear);
            return true;
        }

        let mut fields = payload.splitn(5, '\t');
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
        let Ok(bytes) = STANDARD.decode(encoded) else {
            return true;
        };
        if bytes.len() > MAX_TEXT_BYTES {
            return true;
        }
        let Ok(text) = String::from_utf8(bytes) else {
            return true;
        };
        let _ = self.sender.send(NarratorCommand::Say {
            text,
            interrupt,
            volume,
        });
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

    #[test]
    fn ignores_regular_log_lines() {
        let (sender, _receiver) = mpsc::channel();
        let broker = NarratorBroker { sender };

        assert!(!broker.handle_line("ordinary Minecraft log"));
    }

    #[test]
    fn accepts_protocol_wrapped_by_log4j() {
        let (sender, receiver) = mpsc::channel();
        let broker = NarratorBroker { sender };

        assert!(broker.handle_line("[Render thread/INFO]: [STDOUT]: MONALAUNCHER_NARRATOR\tCLEAR"));
        assert!(matches!(receiver.recv().unwrap(), NarratorCommand::Clear));
    }
}
