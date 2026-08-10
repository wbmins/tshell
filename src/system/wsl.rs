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
/// Reads the Windows registry (no external `wsl.exe` process, no console
/// window, no latency). Each installed distro is a subkey of
/// `HKCU\...\CurrentVersion\Lxss` exposing a `DistributionName` value.
/// On non-Windows platforms this always returns an empty vec.
#[cfg(target_os = "windows")]
pub fn list_wsl_distros() -> Vec<WslDistro> {
    let t_start = std::time::Instant::now();
    let distros = list_wsl_distros_from_registry().unwrap_or_default();
    tracing::info!(
        "[wsl] list_wsl_distros: registry took {:.1}ms, found {} distros",
        t_start.elapsed().as_millis(),
        distros.len()
    );
    distros
}

/// Reads installed WSL distro names from the Windows registry.
///
/// Each installed distro is a subkey of `HKCU\...\Lxss` exposing a
/// `DistributionName` value. This is much faster than launching `wsl.exe` and
/// never flashes a console window.
#[cfg(target_os = "windows")]
fn list_wsl_distros_from_registry() -> Result<Vec<WslDistro>, anyhow::Error> {
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let lxss = hkcu
        .open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Lxss")?;

    let mut distros = Vec::new();
    for sub in lxss.enum_keys().flatten() {
        let key = lxss.open_subkey(&sub)?;
        let name: Option<String> = key.get_value("DistributionName").ok();
        if let Some(name) = name {
            if !name.trim().is_empty() {
                distros.push(WslDistro { name });
            }
        }
    }
    Ok(distros)
}

#[cfg(not(target_os = "windows"))]
pub fn list_wsl_distros() -> Vec<WslDistro> {
    Vec::new()
}

