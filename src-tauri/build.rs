use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=../news.config.json");
    println!("cargo:rerun-if-env-changed=NEWS_SITE_URL");
    println!("cargo:rerun-if-changed=java/narrator-bridge");
    println!("cargo:rerun-if-changed=java/narrator-bridge-smoke");
    println!("cargo:rerun-if-changed=java/cursor-agent");
    println!("cargo:rerun-if-changed=java/cursor-agent-smoke");
    println!("cargo:rerun-if-env-changed=MONALAUNCHER_MICROSOFT_CLIENT_ID");
    println!("cargo:rerun-if-changed=java/auth-bridge");
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if matches!(target_os.as_str(), "windows" | "macos" | "linux") {
        build_narrator_bridge();
        build_auth_bridge(&target_os);
    }
    if target_os == "windows" {
        build_cursor_agent();
    }
    tauri_build::build()
}

fn build_auth_bridge(target_os: &str) {
    let output = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is set"));
    let classes = output.join("auth-bridge-classes");
    std::fs::create_dir_all(&classes).expect("create auth bridge classes");
    let java = Command::new("java")
        .args(["-XshowSettings:properties", "-version"])
        .output()
        .expect("Java development kit required");
    let settings = String::from_utf8_lossy(&java.stderr);
    let java_home = settings
        .lines()
        .find_map(|line| line.trim().strip_prefix("java.home = "))
        .map(PathBuf::from)
        .expect("Java home is available");
    let sources = [
        "AuthAgent.java",
        "AuthBridge.java",
        "MethodAdapter.java",
        "NativeIO.java",
    ];
    let status = Command::new("javac")
        .args(["--release", "21", "-d"])
        .arg(&classes)
        .args(sources.map(|name| format!("java/auth-bridge/me/aomona/auth/{name}")))
        .status()
        .expect("compile authentication adapter");
    assert!(
        status.success(),
        "authentication adapter compilation failed"
    );
    let manifest = output.join("auth-bridge-manifest.mf");
    std::fs::write(
        &manifest,
        "Manifest-Version: 1.0\nPremain-Class: me.aomona.auth.AuthAgent\n\n",
    )
    .unwrap();
    assert!(Command::new("jar")
        .arg("cfm")
        .arg(output.join("auth-bridge.jar"))
        .arg(manifest)
        .arg("-C")
        .arg(&classes)
        .arg("me/aomona/auth/AuthAgent.class")
        .arg("-C")
        .arg(&classes)
        .arg("me/aomona/auth/AuthAgent$1.class")
        .arg("-C")
        .arg(&classes)
        .arg("me/aomona/auth/MethodAdapter.class")
        .status()
        .unwrap()
        .success());
    assert!(Command::new("jar")
        .arg("cf")
        .arg(output.join("auth-bootstrap.jar"))
        .arg("-C")
        .arg(&classes)
        .arg("me/aomona/auth/AuthBridge.class")
        .arg("-C")
        .arg(&classes)
        .arg("me/aomona/auth/NativeIO.class")
        .status()
        .unwrap()
        .success());
    let mut compiler = cc::Build::new();
    compiler.include(java_home.join("include"));
    compiler.include(java_home.join("include").join(if target_os == "windows" {
        "win32"
    } else if target_os == "macos" {
        "darwin"
    } else {
        "linux"
    }));
    let tool = compiler.get_compiler();
    let mut command = tool.to_command();
    let native = if target_os == "windows" {
        "auth-bridge.dll"
    } else if target_os == "macos" {
        "libauth-bridge.dylib"
    } else {
        "libauth-bridge.so"
    };
    if tool.is_like_msvc() {
        command
            .arg("/LD")
            .arg("java/auth-bridge/native.c")
            .arg(format!("/Fe:{}", output.join(native).display()));
    } else {
        command
            .args([
                "-shared",
                "-fPIC",
                "-Wall",
                "-Wextra",
                "-Werror",
                "java/auth-bridge/native.c",
                "-o",
            ])
            .arg(output.join(native));
    }
    assert!(
        command
            .status()
            .expect("compile authentication IPC bridge")
            .success(),
        "authentication IPC bridge compilation failed"
    );
    println!("cargo:rustc-env=MONALAUNCHER_AUTH_NATIVE={native}");
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
    let javac = Command::new("javac")
        .args(["--release", "8", "-cp"])
        .arg(&jar_path)
        .arg("-d")
        .arg(&classes)
        .arg("java/narrator-bridge-smoke/NarratorPolicySmoke.java")
        .status()
        .expect("javac is required for the narrator policy smoke test");
    assert!(javac.success(), "narrator policy smoke compilation failed");
    let classpath = env::join_paths([&jar_path, &classes]).expect("valid Java classpath");
    for enabled in ["true", "false"] {
        let result = Command::new("java")
            .arg("-cp")
            .arg(&classpath)
            .args(["NarratorPolicySmoke", enabled])
            .status()
            .expect("java is required for the narrator policy smoke test");
        assert!(result.success(), "narrator policy smoke failed ({enabled})");
    }
}
