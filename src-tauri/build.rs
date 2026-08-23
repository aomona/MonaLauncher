use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=java/narrator-bridge");
    println!("cargo:rerun-if-env-changed=MONALAUNCHER_MICROSOFT_CLIENT_ID");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        build_narrator_bridge();
    }
    tauri_build::build()
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
