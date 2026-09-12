// Keep the existing Windows probe import path stable.
pub use crate::platform::narrator_broker::NarratorBroker;
use crate::platform::narrator_broker::{NarratorCommand, PlaybackState};
use std::sync::{
    mpsc::{Receiver, SyncSender},
    Arc,
};
use windows::core::PCWSTR;
use windows::Win32::Media::Speech::{
    ISpVoice, SpVoice, SPF_ASYNC, SPF_IS_NOT_XML, SPF_PURGEBEFORESPEAK,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
};

pub(crate) fn run_worker(
    receiver: Receiver<NarratorCommand>,
    ready_sender: SyncSender<Result<(), String>>,
    _playback: Arc<PlaybackState>,
) {
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
