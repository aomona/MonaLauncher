use crate::Result;
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io;
use std::net::{TcpStream, UdpSocket};
use std::path::Path;
use std::process::Command;
use std::time::Duration;

pub const CHECKS: &[(&str, bool)] = &[
    ("read_shared", true),
    ("read_manifest", true),
    ("write_game", true),
    ("read_secret", false),
    ("write_secret", false),
    ("read_other_instance", false),
    ("write_shared", false),
    ("write_manifest", false),
    ("read_symlink_escape", false),
    ("write_symlink_escape", false),
    ("tcp_host_loopback", false),
    ("udp_host_loopback", false),
    ("unix_host_socket", false),
];

pub fn parse(stdout: &[u8]) -> Result<BTreeMap<String, bool>> {
    let mut checks = BTreeMap::new();
    for line in std::str::from_utf8(stdout)?.lines() {
        if let Some(check) = line.strip_prefix("CHECK\t") {
            let (name, value) = check.split_once('\t').ok_or("invalid probe line")?;
            if checks.insert(name.to_owned(), value.parse()?).is_some() {
                return Err(format!("duplicate probe result: {name}").into());
            }
        }
    }
    Ok(checks)
}

pub fn matches(checks: &BTreeMap<String, bool>, sandboxed: bool) -> bool {
    checks.len() == CHECKS.len() * 2
        && ["", "child."].iter().all(|prefix| {
            CHECKS.iter().all(|(name, expected)| {
                checks.get(&format!("{prefix}{name}")) == Some(&(!sandboxed || *expected))
            })
        })
}

pub fn run(args: &[String]) -> Result<()> {
    if args.len() != 4 {
        return Err("probe expects root, TCP port, UDP port, parent|leaf".into());
    }
    let root = Path::new(&args[0]);
    let read = |relative: &str| fs::read(root.join(relative)).map(|_| ());
    let write = |relative: &str| {
        OpenOptions::new()
            .write(true)
            .open(root.join(relative))
            .map(|_| ())
    };
    emit("read_shared", read("shared/asset"));
    emit("read_manifest", read("instance/manifest"));
    emit(
        "write_game",
        fs::write(root.join("game/probe-write"), b"probe"),
    );
    emit("read_secret", read("host/secret"));
    emit("write_secret", write("host/secret"));
    emit("read_other_instance", read("other-instance/manifest"));
    emit("write_shared", write("shared/asset"));
    emit("write_manifest", write("instance/manifest"));
    emit("read_symlink_escape", read("game/escape/secret"));
    emit("write_symlink_escape", write("game/escape/secret"));
    emit(
        "tcp_host_loopback",
        TcpStream::connect_timeout(
            &format!("127.0.0.1:{}", args[1]).parse()?,
            Duration::from_millis(800),
        )
        .map(|_| ()),
    );
    emit("udp_host_loopback", udp_echo(&args[2]));
    #[cfg(unix)]
    emit(
        "unix_host_socket",
        std::os::unix::net::UnixStream::connect(root.join("host/service.sock")).map(|_| ()),
    );
    #[cfg(not(unix))]
    return Err("native fixture currently requires Unix".into());

    if args[3] == "parent" {
        // The descendant receives no new sandbox command: this tests inheritance.
        let child = Command::new(std::env::current_exe()?)
            .arg("--probe")
            .args(&args[..3])
            .arg("leaf")
            .output()?;
        eprint!("{}", String::from_utf8_lossy(&child.stderr));
        if !child.status.success() {
            return Err("descendant probe failed".into());
        }
        for (name, value) in parse(&child.stdout)? {
            println!("CHECK\tchild.{name}\t{value}");
        }
    } else if args[3] != "leaf" {
        return Err("invalid probe depth".into());
    }
    Ok(())
}

fn udp_echo(port: &str) -> io::Result<()> {
    let socket = UdpSocket::bind("127.0.0.1:0")?;
    socket.set_read_timeout(Some(Duration::from_millis(800)))?;
    socket.connect(format!("127.0.0.1:{port}"))?;
    socket.send(b"probe")?;
    let mut reply = [0; 5];
    if socket.recv(&mut reply)? != 5 || &reply != b"probe" {
        return Err(io::Error::other("wrong UDP echo"));
    }
    Ok(())
}

fn emit(name: &str, result: io::Result<()>) {
    if let Err(error) = &result {
        eprintln!("{name}: {error}");
    }
    println!("CHECK\t{name}\t{}", result.is_ok());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_or_duplicate_results_cannot_pass() {
        assert!(!matches(&BTreeMap::new(), true));
        assert!(parse(b"CHECK\tx\ttrue\nCHECK\tx\tfalse\n").is_err());
    }

    #[test]
    fn unconfined_results_cannot_pass_as_isolated() {
        let checks = ["", "child."]
            .iter()
            .flat_map(|prefix| {
                CHECKS
                    .iter()
                    .map(move |(name, _)| (format!("{prefix}{name}"), true))
            })
            .collect();
        assert!(matches(&checks, false));
        assert!(!matches(&checks, true));
    }
}
