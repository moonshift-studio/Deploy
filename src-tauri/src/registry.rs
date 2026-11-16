use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE};
use winreg::RegKey;

use crate::apppack::AppPack;
use crate::util::log_line;

pub fn open_key(pack: &AppPack, create: bool) -> Result<RegKey, String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    if create {
        hkcu.create_subkey(&pack.registry_key)
            .map(|(k, _)| k)
            .map_err(|e| format!("could not create registry key: {e}"))
    } else {
        hkcu.open_subkey_with_flags(&pack.registry_key, KEY_READ)
            .map_err(|e| format!("registry key not found: {e}"))
    }
}

pub fn get_install_dir(pack: &AppPack) -> Option<String> {
    open_key(pack, false)
        .and_then(|k| k.get_value::<String, _>("InstallDir").map_err(|_| "none".to_string()))
        .ok()
}

pub fn get_package_full_name(pack: &AppPack) -> Option<String> {
    open_key(pack, false)
        .and_then(|k| {
            k.get_value::<String, _>("IdentityPackage")
                .map_err(|_| "none".to_string())
        })
        .ok()
}

pub fn set_install_record(
    pack: &AppPack,
    dir: &str,
    version: &str,
    package_full_name: Option<&str>,
) -> Result<(), String> {
    let key = open_key(pack, true)?;
    key.set_value("InstallDir", &dir)
        .map_err(|e| format!("registry write failed: {e}"))?;
    let _ = key.set_value("Version", &version);
    match package_full_name {
        Some(name) => {
            let _ = key.set_value("IdentityPackage", &name);
        }
        None => {
            let _ = key.delete_value("IdentityPackage");
        }
    }
    log_line(&format!("registry record updated: {dir} ({version})"));
    Ok(())
}

pub fn set_autostart(pack: &AppPack, dir: &str) -> Result<(), String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let run = hkcu
        .open_subkey_with_flags(r"Software\Microsoft\Windows\CurrentVersion\Run", KEY_SET_VALUE)
        .map_err(|e| format!("Run key unavailable: {e}"))?;
    let exe = pack.installed_exe(dir);
    run.set_value(&pack.autostart_name, &format!("\"{exe}\""))
        .map_err(|e| format!("autostart write failed: {e}"))?;
    Ok(())
}

pub fn remove_autostart(pack: &AppPack) {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    if let Ok(run) =
        hkcu.open_subkey_with_flags(r"Software\Microsoft\Windows\CurrentVersion\Run", KEY_SET_VALUE)
    {
        let _ = run.delete_value(&pack.autostart_name);
    }
}

pub fn remove_app_key(pack: &AppPack) {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let _ = hkcu.delete_subkey(&pack.registry_key);
}

/// The Add/Remove Programs record (`HKCU\Software\Microsoft\Windows\
/// CurrentVersion\Uninstall\<identityName>`): makes the app appear in
/// Windows Settings → Apps and in the legacy appwiz.cpl list, with an
/// `UninstallString` pointing at the uninstaller copy inside the install
/// directory. `size_kb` feeds the estimated-size column.
pub fn set_uninstall_record(
    pack: &AppPack,
    dir: &str,
    version: &str,
    uninstaller: &str,
    size_kb: u32,
) -> Result<(), String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let arp = r"Software\Microsoft\Windows\CurrentVersion\Uninstall";
    let (key, _) = hkcu
        .create_subkey(format!("{arp}\\{}", pack.identity_name))
        .map_err(|e| format!("could not create the uninstall registry key: {e}"))?;
    let exe = pack.installed_exe(dir);
    let set = |name: &str, value: &str| {
        key.set_value(name, &value.to_string())
            .map_err(|e| format!("could not write {name}: {e}"))
    };
    set("DisplayName", &pack.app_name)?;
    set("DisplayVersion", version)?;
    set("Publisher", &pack.publisher)?;
    set("DisplayIcon", &format!("\"{exe}\""))?;
    set("InstallLocation", dir)?;
    set("UninstallString", &format!("\"{uninstaller}\" /uninstall"))?;
    set("QuietUninstallString", &format!("\"{uninstaller}\" /uninstall /S"))?;
    let _ = key.set_value("NoModify", &1u32);
    let _ = key.set_value("NoRepair", &1u32);
    let _ = key.set_value("EstimatedSize", &size_kb);
    log_line(&format!(
        "uninstall record written: {} -> {uninstaller}",
        pack.identity_name
    ));
    Ok(())
}

pub fn remove_uninstall_record(pack: &AppPack) {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let _ = hkcu.delete_subkey(format!(
        r"Software\Microsoft\Windows\CurrentVersion\Uninstall\{}",
        pack.identity_name
    ));
}
