//! Host-side eSpeak NG worker. Game text is stdin data, never a command or filename.
use crate::platform::narrator_broker::{NarratorCommand, PlaybackState, QUEUE_CAPACITY};
use std::collections::VecDeque;
use std::io::Write;
use std::process::{Child, Command, Stdio};
use std::sync::{
    atomic::Ordering,
    mpsc::{Receiver, RecvTimeoutError, SyncSender},
    Arc,
};
use std::time::{Duration, Instant};

fn stop(child: &mut Option<(Child, Instant)>, state: &PlaybackState) {
    if let Some((mut process, _)) = child.take() {
        let _ = process.kill();
        let _ = process.wait();
        state.stopped.fetch_add(1, Ordering::Relaxed);
    }
    state.speaking.store(false, Ordering::Relaxed);
}
pub(crate) fn run_worker(
    receiver: Receiver<NarratorCommand>,
    ready: SyncSender<Result<(), String>>,
    state: Arc<PlaybackState>,
) {
    if !std::path::Path::new("/usr/bin/espeak-ng").is_file() {
        let _ = ready.send(Err("Linux narrator requires espeak-ng".into()));
        return;
    }
    if ready.send(Ok(())).is_err() {
        return;
    }
    let mut queue = VecDeque::new();
    let mut active: Option<(Child, Instant)> = None;
    loop {
        match receiver.recv_timeout(Duration::from_millis(20)) {
            Ok(NarratorCommand::Clear) => {
                queue.clear();
                stop(&mut active, &state);
            }
            Ok(NarratorCommand::Say {
                text,
                interrupt,
                volume,
            }) => {
                if interrupt {
                    queue.clear();
                    stop(&mut active, &state);
                }
                if queue.len() < QUEUE_CAPACITY && volume > 0.0 {
                    queue.push_back((text, volume));
                }
            }
            Err(RecvTimeoutError::Disconnected) => {
                stop(&mut active, &state);
                break;
            }
            Err(RecvTimeoutError::Timeout) => {}
        }
        if let Some((process, started)) = &mut active {
            match process.try_wait() {
                Ok(Some(status)) => {
                    if status.success() {
                        state.completed.fetch_add(1, Ordering::Relaxed);
                    }
                    active = None;
                    state.speaking.store(false, Ordering::Relaxed);
                }
                Ok(None) if started.elapsed() < Duration::from_secs(60) => {}
                _ => stop(&mut active, &state),
            }
        }
        if active.is_none() {
            if let Some((text, volume)) = queue.pop_front() {
                let result = Command::new("/usr/bin/espeak-ng")
                    .args(["--stdin", "-a"])
                    .arg(((volume * 100.0).round() as u32).to_string())
                    .stdin(Stdio::piped())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn();
                if let Ok(mut child) = result {
                    let written = child
                        .stdin
                        .take()
                        .is_some_and(|mut stdin| stdin.write_all(text.as_bytes()).is_ok());
                    if written {
                        state.started.fetch_add(1, Ordering::Relaxed);
                        state.speaking.store(true, Ordering::Relaxed);
                        active = Some((child, Instant::now()));
                    } else {
                        let _ = child.kill();
                        let _ = child.wait();
                    }
                }
            }
        }
    }
}
