use serde::Serialize;

use crate::apppack::{join_dir, AppPack};
use crate::assets::{APP_VERSION, CERT_CER, IDENTITY_MSIX, PAYLOAD_FILES};
use crate::cert;
use crate::files;
use crate::identity;
use crate::registry;
use crate::util::{log_line, run_hidden};

#[derive(Debug, Clone, Serialize)]
pub struct Progress {
    pub stage: String,
    pub detail: String,
    pub percent: u8,
    pub done: bool,
    pub error: Option<String>,
}

pub struct InstallOptions {
    pub dir: String,
    pub start_menu: bool,
    pub desktop: bool,
    pub autostart: bool,
    pub launch_after: bool,
}

pub type Sink<'a> = &'a mut (dyn FnMut(Progress) + Send);

fn report(sink: Sink, stage: &str, detail: &str, percent: u8, done: bool) {
    log_line(&format!("{stage}: {detail} ({percent}%)"));
    sink(Progress {
        stage: stage.to_string(),
        detail: detail.to_string(),
        percent,
        done,
        error: None,
    });
}

fn report_err(sink: Sink, stage: &str, detail: &str, percent: u8) {
    log_line(&format!("{stage} FAILED: {detail}"));
    sink(Progress {
        stage: stage.to_string(),
        detail: detail.to_string(),
        percent,
        done: true,
        error: Some(detail.to_string()),
    });
}

fn resolve_install_dir(pack: &AppPack, requested: Option<String>) -> String {
    requested
        .or_else(|| registry::get_install_dir(pack))
        .unwrap_or_else(|| pack.resolved_default_dir())
}

/// The rough installed size for the Add/Remove Programs record: every
/// embedded payload file plus the sparse identity package, in KB.
fn payload_size_kb() -> u32 {
    let bytes: usize = PAYLOAD_FILES.iter().map(|f| f.bytes.len()).sum::<usize>()
        + IDENTITY_MSIX.map_or(0, |b| b.len());
    ((bytes + 1023) / 1024).max(1) as u32
}

pub fn install(pack: &AppPack, opts: &InstallOptions, sink: Sink) -> Result<(), String> {
    let t0 = std::time::Instant::now();
    report(sink, "prepare", "Preparing installation", 4, false);

    if PAYLOAD_FILES.is_empty() {
        return Err("no payload embedded in this build".to_string());
    }

    let dir = opts.dir.trim().trim_matches('"').to_string();
    if dir.is_empty() {
        return Err("no install directory chosen".to_string());
    }
    let exe = pack.installed_exe(&dir);
    if std::path::Path::new(&exe).is_file() {
        if !files::kill_processes(&pack.processes, 4) {
            return Err("the app is still running and could not be closed".to_string());
        }
    }
    report(sink, "prepare", "Environment ready", 12, false);

    report(sink, "files", "Copying application files", 18, false);
    files::stage_payload(&dir, PAYLOAD_FILES).map_err(|e| {
        report_err(sink, "files", &e, 20);
        e
    })?;
    let uninstaller = files::stage_uninstaller(&dir).map_err(|e| {
        report_err(sink, "files", &e, 20);
        e
    })?;

    if !CERT_CER.map_or(true, |b| b.is_empty()) {
        report(sink, "cert", "Establishing trust", 34, false);
        let identity_needed = !IDENTITY_MSIX.map_or(true, |b| b.is_empty());
        cert::trust_certificate(CERT_CER.unwrap_or(&[]), &pack.identity_publisher, identity_needed)
            .map_err(|e| {
                report_err(sink, "cert", &e, 38);
                e
            })?;
    }

    let mut package_full_name = None;
    if !IDENTITY_MSIX.map_or(true, |b| b.is_empty()) {
        report(sink, "identity", "Registering with Windows", 52, false);
        let existing = identity::find_registered_package_full_names(&pack.identity_name)
            .unwrap_or_default();
        for name in &existing {
            if let Err(e) = identity::unregister_by_full_name(name) {
                log_line(&format!("ignoring unregister error for {name}: {e}"));
            }
        }
        let msix_dest = files::stage_file(&dir, "identity.msix", IDENTITY_MSIX.unwrap_or(&[])).map_err(|e| {
            report_err(sink, "identity", &e, 56);
            e
        })?;
        let full = identity::register_sparse(&msix_dest, &dir, &pack.identity_name).map_err(|e| {
            report_err(sink, "identity", &e, 60);
            e
        })?;
        package_full_name = Some(full);
    }

    if opts.start_menu {
        report(sink, "shortcuts", "Creating Start menu shortcut", 72, false);
        if let Ok(programs) = files::start_menu_dir() {
            let lnk = join_dir(&programs, &format!("{}.lnk", pack.start_menu_name));
            let icon = pack.installed_exe(&dir);
            let _ = files::create_shortcut(
                &lnk,
                &exe,
                "",
                &dir,
                &pack.description,
                &icon,
            )
            .inspect_err(|e| log_line(e));
            let uninstall_lnk = join_dir(
                &programs,
                &format!("Uninstall {}.lnk", pack.start_menu_name),
            );
            let _ = files::create_shortcut(
                &uninstall_lnk,
                &uninstaller,
                "/uninstall",
                &dir,
                &format!("Remove {}", pack.app_name),
                &uninstaller,
            )
            .inspect_err(|e| log_line(e));
        }
    }
    if opts.desktop {
        if let Ok(desktop) = files::desktop_dir() {
            let lnk = join_dir(&desktop, &format!("{}.lnk", pack.start_menu_name));
            let icon = pack.installed_exe(&dir);
            let _ = files::create_shortcut(
                &lnk,
                &exe,
                "",
                &dir,
                &pack.description,
                &icon,
            )
            .inspect_err(|e| log_line(e));
        }
    }

    report(sink, "registry", "Recording installation", 82, false);
    registry::set_install_record(pack, &dir, APP_VERSION, package_full_name.as_deref())
        .inspect_err(|e| log_line(e))
        .ok();
    registry::set_uninstall_record(pack, &dir, APP_VERSION, &uninstaller, payload_size_kb())
        .inspect_err(|e| log_line(e))
        .ok();

    if opts.autostart {
        let _ = registry::set_autostart(pack, &dir);
    }

    if opts.launch_after {
        report(sink, "launch", "Launching", 94, false);
        files::launch_app(&exe, &dir, &[]).ok();
    }
    report(sink, "done", "Installation complete", 100, true);
    log_line(&format!("install finished in {:?}", t0.elapsed()));
    Ok(())
}

pub fn update(
    pack: &AppPack,
    requested_dir: Option<String>,
    app_args: &[String],
    restart: bool,
    sink: Sink,
) -> Result<(), String> {
    let t0 = std::time::Instant::now();
    report(sink, "prepare", "Checking for the previous installation", 5, false);

    let dir = resolve_install_dir(pack, requested_dir);
    let exe = pack.installed_exe(&dir);
    if !std::path::Path::new(&exe).is_file() {
        let fallback = pack.resolved_default_dir();
        if std::path::Path::new(&pack.installed_exe(&fallback)).is_file() {
            update(pack, Some(fallback), app_args, restart, sink)?;
            return Ok(());
        }
        let e = format!("no previous installation found at {dir}");
        report_err(sink, "prepare", &e, 8);
        return Err(e);
    }

    if !files::kill_processes(&pack.processes, 6) {
        let e = "the app is still running and could not be closed".to_string();
        report_err(sink, "prepare", &e, 10);
        return Err(e);
    }

    report(sink, "files", "Updating application files", 20, false);
    files::stage_payload(&dir, PAYLOAD_FILES).map_err(|e| {
        report_err(sink, "files", &e, 24);
        e
    })?;
    let uninstaller = files::stage_uninstaller(&dir).map_err(|e| {
        report_err(sink, "files", &e, 24);
        e
    })?;

    if !CERT_CER.map_or(true, |b| b.is_empty()) {
        report(sink, "cert", "Verifying trust", 38, false);
        let identity_needed = !IDENTITY_MSIX.map_or(true, |b| b.is_empty());
        cert::trust_certificate(CERT_CER.unwrap_or(&[]), &pack.identity_publisher, identity_needed).ok();
    }

    if !IDENTITY_MSIX.map_or(true, |b| b.is_empty()) {
        report(sink, "identity", "Refreshing Windows registration", 52, false);
        let msix_dest = files::stage_file(&dir, "identity.msix", IDENTITY_MSIX.unwrap_or(&[])).map_err(|e| {
            report_err(sink, "identity", &e, 56);
            e
        })?;
        let existing = identity::find_registered_package_full_names(&pack.identity_name)
            .unwrap_or_default();
        for name in &existing {
            if let Err(e) = identity::unregister_by_full_name(name) {
                log_line(&format!("ignoring unregister error for {name}: {e}"));
            }
        }
        match identity::register_sparse(&msix_dest, &dir, &pack.identity_name) {
            Ok(full) => {
                registry::set_install_record(pack, &dir, APP_VERSION, Some(&full))
                    .inspect_err(|e| log_line(e))
                    .ok();
            }
            Err(e) => {
                if existing.is_empty() {
                    report_err(sink, "identity", &e, 60);
                    return Err(e);
                }
                log_line(&format!("identity re-registration failed, keeping previous: {e}"));
            }
        }
    }

    report(sink, "registry", "Recording update", 74, false);
    registry::set_install_record(
        pack,
        &dir,
        APP_VERSION,
        registry::get_package_full_name(pack).as_deref(),
    )
    .inspect_err(|e| log_line(e))
    .ok();
    registry::set_uninstall_record(pack, &dir, APP_VERSION, &uninstaller, payload_size_kb())
        .inspect_err(|e| log_line(e))
        .ok();

    if restart {
        report(sink, "launch", "Restarting", 90, false);
        files::launch_app(&exe, &dir, app_args)
            .inspect_err(|e| log_line(e))
            .ok();
    }
    report(sink, "done", "Update complete", 100, true);
    log_line(&format!("update finished in {:?}", t0.elapsed()));
    Ok(())
}

pub fn uninstall(pack: &AppPack, sink: Sink) -> Result<(), String> {
    let t0 = std::time::Instant::now();
    report(sink, "prepare", "Locating the installation", 5, false);

    let dir = resolve_install_dir(pack, None);

    report(sink, "identity", "Removing Windows registration", 18, false);
    match registry::get_package_full_name(pack) {
        Some(name) => {
            identity::unregister_by_full_name(&name).ok();
        }
        None => {
            identity::remove_all_registered(&pack.identity_name).ok();
        }
    }

    report(sink, "files", "Deleting files", 42, false);
    files::uninstall_files(pack, &dir);

    report(sink, "registry", "Cleaning up", 72, false);
    registry::remove_uninstall_record(pack);
    registry::remove_autostart(pack);
    registry::remove_app_key(pack);

    let store = &pack.app_name;
    let _ = run_hidden("certutil", &["-user", "-delstore", "TrustedPeople", store]);

    files::cleanup_temp_uninstaller();

    report(sink, "done", "Removed", 100, true);
    log_line(&format!("uninstall finished in {:?}", t0.elapsed()));
    Ok(())
}
