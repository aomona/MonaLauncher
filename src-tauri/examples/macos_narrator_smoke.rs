//! Emits short audible speech and verifies native completion, clearing, and interruption.
#[cfg(target_os = "macos")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use base64::{engine::general_purpose::STANDARD, Engine};
    use monalauncher_lib::probe::NarratorBroker;
    use std::time::{Duration, Instant};

    fn pump() {
        objc2_foundation::NSRunLoop::currentRunLoop().runUntilDate(
            &objc2_foundation::NSDate::dateWithTimeIntervalSinceNow(0.01),
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    fn until(stage: &str, check: impl Fn() -> bool) -> Result<(), Box<dyn std::error::Error>> {
        let deadline = Instant::now() + Duration::from_secs(15);
        while !check() {
            if Instant::now() >= deadline {
                return Err(format!("native speech callback timed out: {stage}").into());
            }
            pump();
        }
        Ok(())
    }

    let mut random = [0u8; 32];
    getrandom::fill(&mut random).map_err(|error| error.to_string())?;
    let token: String = random.iter().map(|b| format!("{b:02x}")).collect();
    let broker = NarratorBroker::start(token.clone())?;
    let say = |text: &str, interrupt: bool| {
        broker.handle_line(&format!(
            "MONALAUNCHER_NARRATOR\t{token}\tSAY\t{}\t0.5\t{}",
            u8::from(interrupt),
            STANDARD.encode(text)
        ));
    };
    let long_text = "MonaLauncher narrator playback test. ".repeat(20);
    say(&long_text, true);
    until("first start", || broker.playback_counts().0 == 1)?;
    eprintln!("First speech started");
    say("This queued message must be cleared.", false);
    broker.handle_line(&format!("MONALAUNCHER_NARRATOR\t{token}\tCLEAR"));
    until("CLEAR cancellation", || broker.stopped_count() == 1)?;
    eprintln!("CLEAR cancelled speech");
    let deadline = Instant::now() + Duration::from_millis(300);
    while Instant::now() < deadline {
        pump();
    }
    assert_eq!(
        broker.playback_counts().0,
        1,
        "CLEAR must remove queued speech"
    );
    assert!(!broker.is_speaking());

    // Separate rate-limit windows so this tests playback rather than throttling.
    let deadline = Instant::now() + Duration::from_secs(1);
    while Instant::now() < deadline {
        pump();
    }
    say(&long_text, true);
    until("second start", || broker.playback_counts().0 == 2)?;
    eprintln!("Second speech started");
    say("ナレーターのテストが完了しました。", true);
    until("interrupt and completion", || {
        broker.stopped_count() == 2
            && broker.playback_counts().0 == 3
            && !broker.is_speaking()
            && broker.playback_counts().1 > 0
    })?;
    println!("PASS: native speech completed; CLEAR discarded queued speech; interrupt cancelled active speech.");
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("macos_narrator_smoke requires macOS");
    std::process::exit(1);
}
