use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=java/narrator-bridge");
    println!("cargo:rerun-if-changed=java/cursor-agent");
    println!("cargo:rerun-if-changed=java/cursor-agent-smoke");
    println!("cargo:rerun-if-env-changed=MONALAUNCHER_MICROSOFT_CLIENT_ID");
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if matches!(target_os.as_str(), "windows" | "macos") {
        build_narrator_bridge();
    }
    if target_os == "windows" {
        build_cursor_agent();
    }
    tauri_build::build()
}

fn build_cursor_agent() {
    let output = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is set by Cargo"));
    let agent_path = output.join("cursor-agent.dll");
    let rustc = env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let compile = Command::new(rustc)
        .args([
            "--crate-name",
            "monalauncher_cursor_agent",
            "--crate-type",
            "cdylib",
            "--edition",
            "2021",
            "-C",
            "opt-level=2",
            "-C",
            "panic=abort",
            "java/cursor-agent/cursor_agent.rs",
            "-o",
        ])
        .arg(&agent_path)
        .status()
        .expect("rustc is required to build the cursor agent");
    assert!(compile.success(), "rustc failed to build the cursor agent");

    smoke_test_cursor_agent(&output, &agent_path);
}

fn smoke_test_cursor_agent(output: &std::path::Path, jar_path: &std::path::Path) {
    let classes = output.join("cursor-agent-smoke-classes");
    if classes.exists() {
        std::fs::remove_dir_all(&classes)
            .expect("failed to clear cursor agent smoke classes directory");
    }
    std::fs::create_dir_all(&classes)
        .expect("failed to create cursor agent smoke classes directory");
    let javac = Command::new("javac")
        .args(["--release", "8", "-d"])
        .arg(&classes)
        .args([
            "java/cursor-agent-smoke/org/lwjgl/glfw/GLFW.java",
            "java/cursor-agent-smoke/org/lwjgl/input/Mouse.java",
            "java/cursor-agent-smoke/CursorAgentSmoke.java",
        ])
        .status()
        .expect("javac is required to build the cursor agent smoke test");
    assert!(
        javac.success(),
        "javac failed to build the cursor agent smoke test"
    );

    const TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    let output = Command::new("java")
        .arg(format!("-agentpath:{}={TOKEN}", jar_path.display()))
        .arg("-cp")
        .arg(&classes)
        .arg("CursorAgentSmoke")
        .output()
        .expect("java is required to smoke test the cursor agent");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success()
            && stdout.contains(&format!("MONALAUNCHER_CURSOR\t{TOKEN}\tGRAB"))
            && stdout.contains(&format!("MONALAUNCHER_CURSOR\t{TOKEN}\tRELEASE")),
        "cursor agent smoke test failed\nstdout: {stdout}\nstderr: {stderr}"
    );
}

fn build_narrator_bridge() {
    let output = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is set by Cargo"));
    let classes = output.join("narrator-bridge-classes");
    if classes.exists() {
        std::fs::remove_dir_all(&classes)
            .expect("failed to clear narrator bridge classes directory");
    }
    std::fs::create_dir_all(&classes).expect("failed to create narrator bridge classes directory");

    let sources = [
        "java/narrator-bridge/com/mojang/text2speech/Narrator.java",
        "java/narrator-bridge/com/mojang/text2speech/LauncherNarrator.java",
    ];
    let javac = Command::new("javac")
        .args(["--release", "8", "-d"])
        .arg(&classes)
        .args(sources)
        .status()
        .expect("javac is required to build the narrator bridge");
    assert!(javac.success(), "javac failed to build the narrator bridge");

    let jar_path = output.join("narrator-bridge.jar");
    let jar = Command::new("jar")
        .args(["cf"])
        .arg(&jar_path)
        .args(["-C"])
        .arg(&classes)
        .arg(".")
        .status()
        .expect("jar is required to package the narrator bridge");
    assert!(jar.success(), "jar failed to package the narrator bridge");
}
