use serde::{Deserialize, Serialize};
use std::time::Duration;

#[cfg(windows)]
use windows::Win32::Foundation::POINT;
#[cfg(windows)]
use windows::Win32::UI::WindowsAndMessaging::{GetCursorPos, SetCursorPos};

#[cfg(windows)]
use monalauncher_lib::probe::{
    current_process_token_info, ensure_appcontainer_profile, launch_probe_in_appcontainer,
    profile_name_for_instance,
};

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProbeResult {
    process_id: u32,
    is_app_container: bool,
    app_container_sid: Option<String>,
    cursor_access: CursorAccessProbe,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct CursorAccessProbe {
    position_readable: bool,
    position_writable: bool,
    write_error: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AppContainerProbeResult {
    parent: ProbeResult,
    child: ProbeResult,
}

#[cfg(windows)]
fn main() {
    let result = match std::env::args().nth(1).as_deref() {
        Some("--appcontainer") => run_appcontainer(),
        Some("--appcontainer-child") => run_child(),
        _ => run(),
    };

    if let Err(error) = result {
        eprintln!("sandbox_probe failed: {error}");
        std::process::exit(1);
    }
}

#[cfg(windows)]
fn run() -> Result<(), Box<dyn std::error::Error>> {
    println!(
        "{}",
        serde_json::to_string_pretty(&current_probe_result()?)?
    );

    Ok(())
}

#[cfg(windows)]
fn run_appcontainer() -> Result<(), Box<dyn std::error::Error>> {
    let instance_id = "sandbox-test";
    let profile_name = profile_name_for_instance(instance_id)?;
    let _profile = ensure_appcontainer_profile(&profile_name)?;
    let executable = std::env::current_exe()?;
    let child = launch_probe_in_appcontainer(&profile_name, &executable)?;
    if !child.stderr.trim().is_empty() {
        return Err(format!("probe child stderr: {}", child.stderr.trim()).into());
    }
    let child_result: ProbeResult = serde_json::from_str(child.stdout.trim())?;
    if child_result.process_id != child.process_id
        || child_result.is_app_container != child.token_info.is_app_container
        || child_result.app_container_sid != child.token_info.app_container_sid
    {
        return Err("probe child report did not match its inspected process token".into());
    }
    let result = AppContainerProbeResult {
        parent: current_probe_result()?,
        child: child_result,
    };

    println!("{}", serde_json::to_string_pretty(&result)?);

    Ok(())
}

#[cfg(windows)]
fn run_child() -> Result<(), Box<dyn std::error::Error>> {
    let token_info = current_process_token_info()?;

    if !token_info.is_app_container {
        return Err("probe child was not launched inside an AppContainer".into());
    }

    println!("{}", serde_json::to_string(&current_probe_result()?)?);

    // 親プロセスが子プロセスのトークンを検査する時間を確保する。
    std::thread::sleep(Duration::from_millis(500));

    Ok(())
}

#[cfg(windows)]
fn current_probe_result() -> Result<ProbeResult, Box<dyn std::error::Error>> {
    let token_info = current_process_token_info()?;

    Ok(ProbeResult {
        process_id: std::process::id(),
        is_app_container: token_info.is_app_container,
        app_container_sid: token_info.app_container_sid,
        cursor_access: probe_cursor_access(),
    })
}

#[cfg(windows)]
fn probe_cursor_access() -> CursorAccessProbe {
    let mut position = POINT::default();
    // SAFETY: position points to writable memory for the duration of the Win32 call.
    let read_result = unsafe { GetCursorPos(&mut position) };
    if let Err(error) = read_result {
        return CursorAccessProbe {
            position_readable: false,
            position_writable: false,
            write_error: Some(error.to_string()),
        };
    }

    // SAFETY: writing the cursor's current coordinates does not change its position and lets the
    // probe verify whether the process has WINSTA_WRITEATTRIBUTES access.
    match unsafe { SetCursorPos(position.x, position.y) } {
        Ok(()) => CursorAccessProbe {
            position_readable: true,
            position_writable: true,
            write_error: None,
        },
        Err(error) => CursorAccessProbe {
            position_readable: true,
            position_writable: false,
            write_error: Some(error.to_string()),
        },
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("sandbox_probe is only supported on Windows");
    std::process::exit(1);
}
