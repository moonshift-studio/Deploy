use crate::util::{log_line, run_hidden};
use std::time::Duration;

/// How many times post-registration enumeration is retried before giving
/// up. The AppX Deployment Service acknowledges a registration before the
/// package becomes visible to enumeration on some machines, so a single
/// immediate check can race against propagation.
const VERIFY_ATTEMPTS: u32 = 5;
const VERIFY_RETRY_DELAY_MS: u64 = 400;

pub struct IdentityPackage {
    pub msix_path: String,
}

/// In-process enumeration of registered packages via WinRT — no subprocess,
/// same user context as the registration itself, no dependency on the
/// PowerShell AppX module being present or functional.
fn find_registered_package_full_names_native(
    identity_name: &str,
) -> Result<Vec<String>, String> {
    use windows::Management::Deployment::PackageManager;

    let manager = PackageManager::new().map_err(|e| format!("PackageManager unavailable: {e}"))?;
    let packages = manager
        .FindPackages()
        .map_err(|e| format!("FindPackages failed: {e}"))?;
    let mut names = Vec::new();
    for package in packages {
        let Ok(id) = package.Id() else { continue };
        let Ok(name) = id.Name() else { continue };
        if name.to_string().eq_ignore_ascii_case(identity_name) {
            if let Ok(full) = id.FullName() {
                names.push(full.to_string());
            }
        }
    }
    Ok(names)
}

fn find_registered_package_full_names_via_powershell(
    identity_name: &str,
) -> Result<Vec<String>, String> {
    let script = format!(
        "Get-AppxPackage -Name '{}' | ForEach-Object {{ $_.PackageFullName }}",
        identity_name.replace('\'', "''")
    );
    let out = run_hidden(
        "powershell.exe",
        &[
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            &script,
        ],
    )?;
    let names: Vec<String> = out
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();
    Ok(names)
}

/// Enumerates registered packages with the given identity name. Prefers the
/// native WinRT path and falls back to `Get-AppxPackage` if it is
/// unavailable.
pub fn find_registered_package_full_names(identity_name: &str) -> Result<Vec<String>, String> {
    match find_registered_package_full_names_native(identity_name) {
        Ok(names) => Ok(names),
        Err(e) => {
            log_line(&format!(
                "native package enumeration failed ({e}); falling back to Get-AppxPackage"
            ));
            find_registered_package_full_names_via_powershell(identity_name)
        }
    }
}

/// Logs the Windows build number for remote debugging.
fn log_windows_build() {
    use winreg::enums::HKEY_LOCAL_MACHINE;
    use winreg::RegKey;
    let Ok(ver) = RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey("SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion")
    else {
        return;
    };
    let build: String = ver.get_value("CurrentBuildNumber").unwrap_or_default();
    let revision: u32 = ver.get_value("UBR").unwrap_or(0);
    let display: String = ver.get_value("DisplayVersion").unwrap_or_default();
    log_line(&format!("windows build {display} {build}.{revision}"));
}

pub fn unregister_by_full_name(full_name: &str) -> Result<(), String> {
    use windows::core::HSTRING;
    use windows::Management::Deployment::PackageManager;

    let manager = PackageManager::new().map_err(|e| format!("PackageManager unavailable: {e}"))?;
    let op = manager
        .RemovePackageAsync(&HSTRING::from(full_name))
        .map_err(|e| format!("RemovePackageAsync failed: {e}"))?;
    op.get()
        .map_err(|e| format!("unregistering {full_name} failed: {e}"))?;
    log_line(&format!("unregistered package {full_name}"));
    Ok(())
}

pub fn register_sparse(
    msix_path: &str,
    external_location: &str,
    identity_name: &str,
) -> Result<String, String> {
    use windows::core::HSTRING;
    use windows::Foundation::Uri;
    use windows::Management::Deployment::{AddPackageOptions, PackageManager};

    let manager = PackageManager::new().map_err(|e| format!("PackageManager unavailable: {e}"))?;
    let options =
        AddPackageOptions::new().map_err(|e| format!("AddPackageOptions unavailable: {e}"))?;
    let external_uri =
        Uri::CreateUri(&HSTRING::from(external_location)).map_err(|e| format!("bad location: {e}"))?;
    options
        .SetExternalLocationUri(&external_uri)
        .map_err(|e| format!("bad external location: {e}"))?;
    let package_uri =
        Uri::CreateUri(&HSTRING::from(msix_path)).map_err(|e| format!("bad msix path: {e}"))?;

    let op = manager
        .AddPackageByUriAsync(&package_uri, &options)
        .map_err(|e| format!("AddPackageByUriAsync failed: {e}"))?;
    // The operation completes with an OK status even when the deployment
    // failed — the real error rides in the DeploymentResult (its extended
    // error code and text), so it must be inspected explicitly.
    let result = op.get().map_err(|e| format!("package registration failed: {e}"))?;
    if let Err(status) = result.ExtendedErrorCode() {
        let text = result
            .ErrorText()
            .map(|t| t.to_string())
            .unwrap_or_default();
        return Err(format!(
            "package registration failed: {status}{}",
            if text.is_empty() { String::new() } else { format!(" ({text})") }
        ));
    }

    // Registration completed, but the package can lag a moment behind in
    // enumeration (AppXSvc propagation), so poll before concluding that the
    // registration silently failed.
    let mut full_name = None;
    for attempt in 1..=VERIFY_ATTEMPTS {
        match find_registered_package_full_names(identity_name) {
            Ok(names) if !names.is_empty() => {
                full_name = Some(names.into_iter().next().unwrap());
                break;
            }
            Ok(_) => log_line(&format!(
                "registration verification attempt {attempt}/{VERIFY_ATTEMPTS}: package not yet enumerable"
            )),
            Err(e) => log_line(&format!(
                "registration verification attempt {attempt}/{VERIFY_ATTEMPTS} errored: {e}"
            )),
        }
        if attempt < VERIFY_ATTEMPTS {
            std::thread::sleep(Duration::from_millis(VERIFY_RETRY_DELAY_MS));
        }
    }
    let full_name = match full_name {
        Some(name) => name,
        None => {
            log_windows_build();
            log_line("enumeration still empty after retries; one final Get-AppxPackage check");
            find_registered_package_full_names_via_powershell(identity_name)?
                .into_iter()
                .next()
                .ok_or_else(|| "registration reported success but package not found".to_string())?
        }
    };
    log_line(&format!("registered package {full_name} at {external_location}"));
    Ok(full_name)
}

pub fn remove_all_registered(identity_name: &str) -> Result<usize, String> {
    let names = find_registered_package_full_names(identity_name)?;
    let mut removed = 0;
    for name in &names {
        if unregister_by_full_name(name).is_ok() {
            removed += 1;
        }
    }
    Ok(removed)
}
