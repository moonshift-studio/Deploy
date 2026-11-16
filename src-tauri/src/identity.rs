use crate::util::{log_line, run_hidden};

pub struct IdentityPackage {
    pub msix_path: String,
}

pub fn find_registered_package_full_names(identity_name: &str) -> Result<Vec<String>, String> {
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

    let registered = find_registered_package_full_names(identity_name)?;
    let full_name = registered
        .first()
        .cloned()
        .ok_or_else(|| "registration reported success but package not found".to_string())?;
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
