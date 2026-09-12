use serde::{Deserialize, Serialize};
#[cfg(windows)]
use std::ffi::OsString;
#[cfg(windows)]
use std::fs;
#[cfg(windows)]
use std::io::{Read, Write};
#[cfg(windows)]
use std::path::{Path, PathBuf};
#[cfg(windows)]
use std::process::Command;
use std::time::Duration;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
use windows::Win32::Foundation::POINT;
#[cfg(windows)]
use windows::Win32::UI::WindowsAndMessaging::{GetCursorPos, SetCursorPos};

#[cfg(windows)]
use monalauncher_lib::minecraft::model::{InstanceManifest, ModLoader};
#[cfg(windows)]
use monalauncher_lib::minecraft::paths::MinecraftPaths;
#[cfg(windows)]
use monalauncher_lib::probe::{
    current_process_token_info, ensure_appcontainer_profile, grant_policy_access,
    launch_in_appcontainer, launch_probe_in_appcontainer, lock_sandbox_launch_directory,
    profile_name_for_instance, SandboxDrive,
};

#[cfg(windows)]
const PROBE_RUNTIME_CHECKSUM: &str =
    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

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

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct AclProbeResult {
    manifest_readable: bool,
    manifest_writable: bool,
    fabric_profile_writable: bool,
    mod_registry_writable: bool,
    game_writable: bool,
    launch_directory_writable: bool,
    shared_file_readable: bool,
    shared_file_writable: bool,
    java_readable: bool,
    java_writable: bool,
    other_manifest_readable: bool,
}

#[cfg(windows)]
fn main() {
    let result = match std::env::args().nth(1).as_deref() {
        Some("--appcontainer") => run_appcontainer(),
        Some("--appcontainer-child") => run_child(),
        Some("--acl") => run_acl_probe(),
        Some("--acl-child") => run_acl_child(),
        Some("--job-kill-parent") => run_job_kill_parent(),
        Some("--job-kill-child") => run_job_kill_child(),
        _ => run(),
    };

    if let Err(error) = result {
        eprintln!("sandbox_probe failed: {error}");
        std::process::exit(1);
    }
}

#[cfg(windows)]
fn run_job_kill_parent() -> Result<(), Box<dyn std::error::Error>> {
    let profile_name = profile_name_for_instance("sandbox-test")?;
    let _profile = ensure_appcontainer_profile(&profile_name)?;
    let executable = std::env::current_exe()?;
    let child = launch_in_appcontainer(
        &profile_name,
        &executable,
        &[OsString::from("--job-kill-child")],
        executable.parent().unwrap_or_else(|| Path::new(".")),
    )?;
    println!("{}", child.id());
    std::io::stdout().flush()?;
    // `process::exit` intentionally skips Rust destructors. Windows still closes the job handle,
    // which must terminate the child through JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE.
    std::mem::forget(child);
    std::process::exit(0);
}

#[cfg(windows)]
fn run_job_kill_child() -> Result<(), Box<dyn std::error::Error>> {
    if !current_process_token_info()?.is_app_container {
        return Err("job probe child is not in an AppContainer".into());
    }
    std::thread::sleep(Duration::from_secs(30));
    Ok(())
}

#[cfg(windows)]
fn run_acl_probe() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::temp_dir().join(format!(
        "monalauncher-acl-probe-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos()
    ));
    let result = run_acl_probe_in(&root);
    let _ = fs::remove_dir_all(&root);
    result
}

#[cfg(windows)]
fn run_acl_probe_in(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let paths = MinecraftPaths::new(root.to_owned());
    let instance_id = "acl-test";
    let other_instance_id = "acl-other";
    let java = paths
        .runtimes()
        .join("temurin-21")
        .join(PROBE_RUNTIME_CHECKSUM)
        .join("runtime")
        .join("bin")
        .join("java.exe");
    for directory in [
        paths.assets(),
        paths.libraries(),
        paths.version_directory("1.21.8"),
        paths.instance_game_directory(instance_id),
        paths.instance(other_instance_id),
        java.parent().ok_or("Java path has no parent")?.to_owned(),
    ] {
        fs::create_dir_all(directory)?;
    }
    fs::write(paths.versions().join("shared.txt"), b"shared")?;
    fs::write(&java, b"java")?;
    let instance = InstanceManifest {
        id: instance_id.to_owned(),
        name: "ACL probe".to_owned(),
        version_id: "1.21.8".to_owned(),
        java_path: java.to_string_lossy().into_owned(),
        game_directory: paths
            .instance_game_directory(instance_id)
            .to_string_lossy()
            .into_owned(),
        demo: true,
        sandboxed: true,
        permissions: Default::default(),
        mod_loader: ModLoader::Vanilla,
    };
    fs::write(
        paths.instance_manifest(instance_id),
        serde_json::to_vec(&instance)?,
    )?;
    fs::write(paths.instance_fabric_profile(instance_id), b"{}")?;
    fs::write(paths.instance_mod_registry(instance_id), b"{}")?;
    fs::write(
        paths.instance_manifest(other_instance_id),
        br#"{"id":"acl-other"}"#,
    )?;
    let launch_directory = paths.instance(instance_id).join("sandbox-launches/probe");
    fs::create_dir_all(launch_directory.join("tmp"))?;

    let profile_name = profile_name_for_instance(instance_id)?;
    let profile = ensure_appcontainer_profile(&profile_name)?;
    // Reproduce the broad inheriting grant used by older launcher releases. The secure grant
    // below must replace it, including on existing metadata files, rather than merely adding a
    // narrower ACE alongside it.
    grant_legacy_instance_access(&paths.instance(instance_id), &profile.sid)?;
    let policy = monalauncher_lib::minecraft::sandbox_policy::policy_for_instance(
        &paths,
        &instance,
        &launch_directory,
    )?;
    grant_policy_access(&policy, &profile.sid)?;
    lock_sandbox_launch_directory(&launch_directory, &profile.sid)?;

    let executable = std::env::current_exe()?;
    let drive = SandboxDrive::create(root)?;
    let alias_root = drive.root().to_owned();
    let alias_launch_directory = alias_root.join("instances/acl-test/sandbox-launches/probe");
    let arguments = [
        OsString::from("--acl-child"),
        alias_root.as_os_str().to_owned(),
        OsString::from(instance_id),
        OsString::from(other_instance_id),
        alias_launch_directory.as_os_str().to_owned(),
    ];
    let mut child = launch_in_appcontainer(
        &profile_name,
        &executable,
        &arguments,
        executable.parent().unwrap_or_else(|| Path::new(".")),
    )?;
    child.retain_sandbox_drive(drive);
    let mut stdout = child
        .take_stdout()
        .ok_or("ACL probe stdout is unavailable")?;
    let mut stderr = child
        .take_stderr()
        .ok_or("ACL probe stderr is unavailable")?;
    let status = child.wait()?;
    let mut stdout_text = String::new();
    let mut stderr_text = String::new();
    stdout.read_to_string(&mut stdout_text)?;
    stderr.read_to_string(&mut stderr_text)?;
    if !status.success() || !stderr_text.trim().is_empty() {
        return Err(format!("ACL probe child failed ({status}): {}", stderr_text.trim()).into());
    }
    let report: AclProbeResult = serde_json::from_str(stdout_text.trim())?;
    let expected = report.manifest_readable
        && !report.manifest_writable
        && !report.fabric_profile_writable
        && !report.mod_registry_writable
        && report.game_writable
        && report.launch_directory_writable
        && report.shared_file_readable
        && !report.shared_file_writable
        && report.java_readable
        && !report.java_writable
        && !report.other_manifest_readable;
    if !expected {
        return Err(format!("ACL least-privilege invariant failed: {report:?}").into());
    }
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

#[cfg(windows)]
fn grant_legacy_instance_access(
    instance_directory: &Path,
    appcontainer_sid: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let principal = format!("*{appcontainer_sid}:(OI)(CI)M");
    let output = Command::new("icacls.exe")
        .arg(instance_directory)
        .args(["/grant", &principal, "/Q"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "failed to prepare legacy ACL: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(())
}

#[cfg(windows)]
fn run_acl_child() -> Result<(), Box<dyn std::error::Error>> {
    let arguments = std::env::args_os().skip(2).collect::<Vec<_>>();
    if arguments.len() != 4 {
        return Err("ACL child requires root, instance IDs, and launch directory".into());
    }
    let paths = MinecraftPaths::new(PathBuf::from(&arguments[0]));
    let instance_id = arguments[1].to_string_lossy();
    let other_instance_id = arguments[2].to_string_lossy();
    let launch_directory = PathBuf::from(&arguments[3]);
    let java = paths
        .runtimes()
        .join("temurin-21")
        .join(PROBE_RUNTIME_CHECKSUM)
        .join("runtime/bin/java.exe");
    let report = AclProbeResult {
        manifest_readable: fs::read(paths.instance_manifest(&instance_id)).is_ok(),
        manifest_writable: can_write_existing(&paths.instance_manifest(&instance_id)),
        fabric_profile_writable: can_write_existing(&paths.instance_fabric_profile(&instance_id)),
        mod_registry_writable: can_write_existing(&paths.instance_mod_registry(&instance_id)),
        game_writable: fs::write(
            paths
                .instance_game_directory(&instance_id)
                .join("probe-write.txt"),
            b"game",
        )
        .is_ok(),
        launch_directory_writable: fs::write(launch_directory.join("probe-write.txt"), b"launch")
            .is_ok(),
        shared_file_readable: fs::read(paths.versions().join("shared.txt")).is_ok(),
        shared_file_writable: can_write_existing(&paths.versions().join("shared.txt")),
        java_readable: fs::read(&java).is_ok(),
        java_writable: can_write_existing(&java),
        other_manifest_readable: fs::read(paths.instance_manifest(&other_instance_id)).is_ok(),
    };
    println!("{}", serde_json::to_string(&report)?);
    Ok(())
}

#[cfg(windows)]
fn can_write_existing(path: &Path) -> bool {
    fs::OpenOptions::new()
        .write(true)
        .open(path)
        .and_then(|mut file| file.write_all(b"x"))
        .is_ok()
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
