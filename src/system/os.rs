//! Detection of the OS running inside a connected session.
//!
//! For SSH sessions we connect through the in-process russh client (so no
//! extra `ssh.exe` / cmd window is spawned) and read `/etc/os-release`.
//! For WSL sessions we infer the OS directly from the distro name. The result
//! is a normalized lower-case identifier such as `debian`, `ubuntu`, `alpine`,
//! `android`, `arch`, or `unknown`.

use tokio::runtime::Handle;

use crate::session::config::Session;

/// The shell command run on the target to identify the OS.
///
/// Tries `/etc/os-release` first. Many Android devices lack it, so we fall
/// back to `uname -a` which reports `Android` on such devices.
const DETECT_CMD: &str = r#"sh -c 'ID="$(cat /etc/os-release 2>/dev/null | grep -m1 "^ID=" | cut -d= -f2 | tr -d "\"")"; if [ -n "$ID" ]; then echo "$ID"; elif uname -a 2>/dev/null | grep -qi android; then echo android; else echo unknown; fi'"#;

/// Returns the detected OS identifier for the given session, or "unknown".
///
/// `handle` is the Tokio runtime handle used to drive the russh connection.
pub fn detect_session_os(session: &Session, handle: &Handle) -> String {
    // WSL sessions drop into a specific distro; we can infer the OS directly
    // from the distro name without running any remote command.
    if session.protocol == "wsl" {
        return infer_os_from_name(&session.host);
    }

    let raw = match session.protocol.as_str() {
        "serial" => String::new(), // serial backends can't run shell commands
        _ => detect_remote_ssh(session, handle),
    };

    if raw.is_empty() {
        return "unknown".to_string();
    }

    let id = raw.trim().trim_matches('"').to_lowercase();
    match id.as_str() {
        "debian" | "ubuntu" | "alpine" | "android" | "arch" | "archlinux" => {
            normalize_os(&id)
        }
        "postmarketos" | "postmarket" => "postmarketos".to_string(),
        // Android devices often report `ID=android-x86` or similar.
        _ if id.contains("android") => "android".to_string(),
        _ if id.contains("ubuntu") => "ubuntu".to_string(),
        _ if id.contains("debian") => "debian".to_string(),
        _ if id.contains("alpine") => "alpine".to_string(),
        _ if id.contains("arch") => "arch".to_string(),
        _ if id.contains("postmarket") => "postmarketos".to_string(),
        _ => "unknown".to_string(),
    }
}

/// Guess the OS type from a WSL distro name (e.g. "Ubuntu", "Debian",
/// "ArchLinux", "Alpine", ...).
fn infer_os_from_name(name: &str) -> String {
    let lower = name.to_lowercase();
    if lower.contains("ubuntu") {
        "ubuntu".to_string()
    } else if lower.contains("debian") {
        "debian".to_string()
    } else if lower.contains("alpine") {
        "alpine".to_string()
    } else if lower.contains("android") {
        "android".to_string()
    } else if lower.contains("arch") {
        "arch".to_string()
    } else {
        "unknown".to_string()
    }
}

fn normalize_os(id: &str) -> String {
    match id {
        "archlinux" => "arch".to_string(),
        other => other.to_string(),
    }
}

fn detect_remote_ssh(session: &Session, handle: &Handle) -> String {
    // Use the in-process russh client so no separate ssh.exe (and therefore
    // no cmd window) is spawned. `block_on` is safe from any thread; it drives
    // the reactor associated with the handle.
    handle
        .block_on(crate::backend::ssh::execute_remote_command(session, DETECT_CMD))
        .unwrap_or_default()
}
