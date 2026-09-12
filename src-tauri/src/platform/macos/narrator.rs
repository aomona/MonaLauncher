//! Trusted-host speech. Minecraft only sends bounded text over its existing stdout pipe.
use std::collections::VecDeque;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{Receiver, RecvTimeoutError, SyncSender};
use std::sync::Arc;
use std::time::Duration;

use objc2::rc::autoreleasepool;
use objc2::runtime::ProtocolObject;
use objc2::{define_class, msg_send, AnyThread, DefinedClass};
use objc2_avf_audio::{
    AVSpeechBoundary, AVSpeechSynthesizer, AVSpeechSynthesizerDelegate, AVSpeechUtterance,
};
use objc2_foundation::{NSDate, NSObject, NSObjectProtocol, NSRunLoop, NSString};

use crate::platform::narrator_broker::{NarratorCommand, PlaybackState, QUEUE_CAPACITY};

define_class!(
    // SAFETY: NSObject has no subclassing requirements. The ivars contain only thread-safe atomics.
    #[unsafe(super = NSObject)]
    #[ivars = Arc<PlaybackState>]
    struct NarratorDelegate;

    // SAFETY: NSObjectProtocol has no additional safety requirements.
    unsafe impl NSObjectProtocol for NarratorDelegate {}

    // SAFETY: The implemented selectors and argument types match AVSpeechSynthesizerDelegate.
    unsafe impl AVSpeechSynthesizerDelegate for NarratorDelegate {
        #[unsafe(method(speechSynthesizer:didStartSpeechUtterance:))]
        fn did_start(&self, _voice: &AVSpeechSynthesizer, _utterance: &AVSpeechUtterance) {
            self.ivars().started.fetch_add(1, Ordering::Relaxed);
        }

        #[unsafe(method(speechSynthesizer:didFinishSpeechUtterance:))]
        fn did_finish(&self, _voice: &AVSpeechSynthesizer, _utterance: &AVSpeechUtterance) {
            self.ivars().completed.fetch_add(1, Ordering::Relaxed);
        }


    }
);

pub(crate) fn run_worker(
    receiver: Receiver<NarratorCommand>,
    ready: SyncSender<Result<(), String>>,
    playback: Arc<PlaybackState>,
) {
    autoreleasepool(|_| {
        let allocated = NarratorDelegate::alloc().set_ivars(Arc::clone(&playback));
        // SAFETY: The allocated NSObject subclass has initialized ivars and a valid init method.
        let delegate: objc2::rc::Retained<NarratorDelegate> =
            unsafe { msg_send![super(allocated), init] };
        // SAFETY: All synthesizer access stays on this worker. The delegate is retained until shutdown.
        let voice = unsafe {
            let voice = AVSpeechSynthesizer::new();
            voice.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
            voice
        };
        if ready.send(Ok(())).is_err() {
            return;
        }
        let mut pending = VecDeque::new();
        let run_loop = NSRunLoop::currentRunLoop();
        loop {
            let command = match receiver.recv_timeout(Duration::from_millis(10)) {
                Ok(command) => Some(command),
                Err(RecvTimeoutError::Timeout) => None,
                Err(RecvTimeoutError::Disconnected) => break,
            };
            autoreleasepool(|_| {
                // SAFETY: The voice and utterances are used only on their owning worker; volume
                // and text have already passed the shared protocol's bounds and validation.
                unsafe {
                    match command {
                        Some(NarratorCommand::Clear) => {
                            pending.clear();
                            stop(&voice, &playback);
                        }
                        Some(NarratorCommand::Say {
                            text,
                            interrupt,
                            volume,
                        }) => {
                            if interrupt {
                                pending.clear();
                                stop(&voice, &playback);
                            }
                            if pending.len() < QUEUE_CAPACITY {
                                pending.push_back((text, volume));
                            }
                        }
                        None => {}
                    }
                    // Keep the native synthesizer queue bounded too, not just the IPC channel.
                    if !voice.isSpeaking() {
                        if let Some((text, volume)) = pending.pop_front() {
                            let utterance = AVSpeechUtterance::initWithString(
                                AVSpeechUtterance::alloc(),
                                &NSString::from_str(&text),
                            );
                            utterance.setVolume(volume);
                            voice.speakUtterance(&utterance);
                        }
                    }
                    playback
                        .speaking
                        .store(voice.isSpeaking(), Ordering::Relaxed);
                }
                run_loop.runUntilDate(&NSDate::dateWithTimeIntervalSinceNow(0.01));
            });
        }
        // SAFETY: The worker still owns voice/delegate. Stop playback when the game pipe closes.
        unsafe {
            stop(&voice, &playback);
            voice.setDelegate(None);
        }
    });
}

fn stop(voice: &AVSpeechSynthesizer, playback: &PlaybackState) {
    // SAFETY: Called only on the worker owning the synthesizer. macOS may omit didCancel
    // for early cancellation, so confirm successful stop against the native playback state.
    unsafe {
        if voice.stopSpeakingAtBoundary(AVSpeechBoundary::Immediate) && !voice.isSpeaking() {
            playback.stopped.fetch_add(1, Ordering::Relaxed);
        }
    }
}
