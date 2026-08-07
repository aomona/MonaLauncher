#[cfg(windows)]
use std::ffi::OsString;
#[cfg(windows)]
use std::io::Read;
#[cfg(windows)]
use std::path::PathBuf;
#[cfg(windows)]
use std::process::Command;

#[cfg(windows)]
use monalauncher_lib::minecraft::{paths::MinecraftPaths, runtime::install_java_21_runtime};
#[cfg(windows)]
use monalauncher_lib::probe::{
    ensure_appcontainer_profile, launch_in_appcontainer, profile_name_for_instance,
};

#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .ok_or("APPDATA is unavailable")?
        .join("me.aomona.monalauncher")
        .join("minecraft");
    let paths = MinecraftPaths::new(root);
    let java = match std::env::var_os("MONALAUNCHER_JAVA_SMOKE") {
        Some(path) => PathBuf::from(path),
        None => install_java_21_runtime(&paths, |progress| {
            println!("[runtime] {}", progress.message);
        })?,
    };
    let java_root = java
        .parent()
        .and_then(|path| path.parent())
        .ok_or("Java runtime root is unavailable")?;
    let profile_name = profile_name_for_instance("smoke-demo")?;
    let profile = ensure_appcontainer_profile(&profile_name)?;
    let principal = format!("*{}:(OI)(CI)RX", profile.sid);
    let acl = Command::new("icacls.exe")
        .arg(java_root)
        .args(["/grant", &principal, "/Q"])
        .output()?;
    if !acl.status.success() {
        return Err(format!(
            "icacls failed: {}{}",
            String::from_utf8_lossy(&acl.stdout),
            String::from_utf8_lossy(&acl.stderr)
        )
        .into());
    }

    let mut child = launch_in_appcontainer(
        &profile_name,
        &java,
        &[OsString::from("-version")],
        java_root,
    )?;
    println!(
        "token: isAppContainer={}, sid={:?}",
        child.token_info.is_app_container, child.token_info.app_container_sid
    );
    if !child.token_info.is_app_container {
        return Err("Java did not receive an AppContainer token".into());
    }

    let mut child_stdout = child.take_stdout().ok_or("Java stdout is unavailable")?;
    let mut child_stderr = child.take_stderr().ok_or("Java stderr is unavailable")?;
    let status = child.wait()?;
    let mut stdout = String::new();
    let mut stderr = String::new();
    child_stdout.read_to_string(&mut stdout)?;
    child_stderr.read_to_string(&mut stderr)?;
    print!("{stdout}{stderr}");
    if !status.success() {
        return Err(format!("Java exited with {status}").into());
    }

    println!("AppContainer Java smoke test passed");
    Ok(())
}

#[cfg(not(windows))]
fn main() {
    eprintln!("AppContainer is only available on Windows");
    std::process::exit(1);
}
