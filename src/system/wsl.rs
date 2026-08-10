//! Scanning of locally installed WSL (Windows Subsystem for Linux) distros.
//!
//! WSL is only available on Windows hosts, so these helpers are gated behind
//! `#[cfg(target_os = "windows")]`. On other platforms an empty list is
//! returned.

/// The name of a single installed WSL distribution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WslDistro {
    pub name: String,
}

/// List all locally installed WSL distributions.
///
/// Runs `wsl.exe -l -q` and parses each non-empty line as a distro name.
/// On non-Windows platforms this always returns an empty vec.
#[cfg(target_os = "windows")]
pub fn list_wsl_distros() -> Vec<WslDistro> {
    let output = match std::process::Command::new("wsl.exe")
        .args(["-l", "-q"])
        .output()
    {
        Ok(o) if o.status.success() => o,
        _ => return Vec::new(),
    };

    // `wsl.exe -l -q` may output UTF-16LE on some systems; try both encodings.
    let raw = output.stdout;
    let text = decode_wsl_output(&raw);

    text.lines()
        .map(|line| line.trim().trim_matches('\u{0}').trim())
        .filter(|line| !line.is_empty() && !line.eq_ignore_ascii_case("NAME"))
        .map(|name| WslDistro {
            name: name.to_string(),
        })
        .collect()
}

#[cfg(not(target_os = "windows"))]
pub fn list_wsl_distros() -> Vec<WslDistro> {
    Vec::new()
}

#[cfg(target_os = "windows")]
fn decode_wsl_output(bytes: &[u8]) -> String {
    // WSL commonly emits UTF-16LE. Detect BOM or NUL-bytes and decode accordingly.
    if bytes.starts_with(&[0xff, 0xfe]) || bytes.starts_with(&[0xfe, 0xff]) {
        return String::from_utf16_lossy(
            &bytes[2..]
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect::<Vec<u16>>(),
        );
    }
    // Heuristic: presence of NUL bytes suggests UTF-16.
    if bytes.windows(2).any(|w| w[0] == 0 || w[1] == 0) {
        let units: Vec<u16> = bytes
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        return String::from_utf16_lossy(&units);
    }
    String::from_utf8_lossy(bytes).to_string()
}
