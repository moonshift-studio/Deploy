use std::fs;
use std::time::Duration;

use windows::core::Interface;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, IPersistFile,
};
use windows::Win32::UI::Shell::{
    IShellLinkW, SHGetKnownFolderPath, ShellExecuteW, ShellLink, FOLDERID_Desktop,
    FOLDERID_Programs, KNOWN_FOLDER_FLAG,
};
use windows::Win32::UI::WindowsAndMessaging::SW_SHOW;

use crate::apppack::{join_dir, AppPack};
use crate::util::{log_line, run_hidden};

pub fn kill_processes(processes: &[String], retries: u32) -> bool {
    for attempt in 0..retries {
        let mut alive = false;
        for name in processes {
            if process_running(name) {
                alive = true;
                let _ = run_hidden("taskkill", &["/IM", name, "/F", "/T"]);
            }
        }
        if !alive {
            return true;
        }
        std::thread::sleep(Duration::from_millis(700));
        log_line(&format!("kill attempt {attempt} left processes running"));
    }
    let still: Vec<&String> = processes.iter().filter(|p| process_running(p)).collect();
    still.is_empty()
}

fn process_running(name: &str) -> bool {
    let filter = format!("imagename eq {name}");
    match run_hidden("tasklist", &["/FI", &filter, "/NH"]) {
        Ok(out) => out.to_ascii_lowercase().contains(&name.to_ascii_lowercase()),
        Err(_) => false,
    }
}

pub fn stage_payload(dir: &str, files: &[crate::assets::PayloadFile]) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|e| format!("could not create {dir}: {e}"))?;
    for f in files {
        let dest = join_dir(dir, f.name);
        fs::write(&dest, f.bytes).map_err(|e| {
            format!("could not write {} ({e}) - is the app still running?", f.name)
        })?;
        log_line(&format!("staged {} ({} bytes)", f.name, f.bytes.len()));
    }
    Ok(())
}

pub fn stage_file(dir: &str, name: &str, bytes: &[u8]) -> Result<String, String> {
    fs::create_dir_all(dir).map_err(|e| format!("could not create {dir}: {e}"))?;
    let dest = join_dir(dir, name);
    fs::write(&dest, bytes).map_err(|e| format!("could not write {name}: {e}"))?;
    Ok(dest)
}

/// Copies the running installer itself into the install directory as
/// `uninstall.exe`: a durable uninstall entry point for the Add/Remove
/// Programs registration and the Start menu uninstall shortcut (the setup
/// exe's original location — Downloads, a mount, a temp dir — is not).
pub fn stage_uninstaller(dir: &str) -> Result<String, String> {
    let src = std::env::current_exe()
        .map_err(|e| format!("could not locate the running installer: {e}"))?;
    fs::create_dir_all(dir).map_err(|e| format!("could not create {dir}: {e}"))?;
    let dest = join_dir(dir, "uninstall.exe");
    fs::copy(&src, &dest).map_err(|e| format!("could not write the uninstaller copy: {e}"))?;
    log_line(&format!("staged uninstaller copy at {dest}"));
    Ok(dest)
}

pub fn start_menu_dir() -> Result<String, String> {
    let p = unsafe { SHGetKnownFolderPath(&FOLDERID_Programs, KNOWN_FOLDER_FLAG(0), None) }
        .map_err(|e| format!("no start menu folder: {e}"))?;
    Ok(unsafe { p.to_string().map_err(|_| "bad start menu path")?.to_string() })
}

pub fn desktop_dir() -> Result<String, String> {
    let p = unsafe { SHGetKnownFolderPath(&FOLDERID_Desktop, KNOWN_FOLDER_FLAG(0), None) }
        .map_err(|e| format!("no desktop folder: {e}"))?;
    Ok(unsafe { p.to_string().map_err(|_| "bad desktop path")?.to_string() })
}

pub fn create_shortcut(
    lnk_path: &str,
    target: &str,
    arguments: &str,
    work_dir: &str,
    description: &str,
    icon_path: &str,
) -> Result<(), String> {
    let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };

    let link: IShellLinkW = unsafe {
        CoCreateInstance(
            &ShellLink,
            None,
            CLSCTX_INPROC_SERVER,
        )
    }
    .map_err(|e| format!("CoCreateInstance failed: {e}"))?;

    unsafe {
        link.SetPath(&windows::core::HSTRING::from(target))
            .map_err(|e| format!("SetPath failed: {e}"))?;
        if !arguments.is_empty() {
            link.SetArguments(&windows::core::HSTRING::from(arguments)).ok();
        }
        link.SetWorkingDirectory(&windows::core::HSTRING::from(work_dir)).ok();
        link.SetDescription(&windows::core::HSTRING::from(description)).ok();
        if !icon_path.is_empty() {
            link.SetIconLocation(&windows::core::HSTRING::from(icon_path), 0).ok();
        }
    }

    let persist: IPersistFile = link
        .cast()
        .map_err(|e| format!("shortcut persist failed: {e}"))?;
    unsafe {
        persist
            .Save(&windows::core::HSTRING::from(lnk_path), false)
            .map_err(|e| format!("saving shortcut failed: {e}"))?;
    }
    Ok(())
}

pub fn delete_if_exists(path: &str) {
    match fs::symlink_metadata(path) {
        Ok(md) if md.is_dir() => {
            let _ = fs::remove_dir_all(path);
        }
        Ok(_) => {
            let _ = fs::remove_file(path);
        }
        Err(_) => {}
    }
}

pub fn launch_app(exe: &str, work_dir: &str, args: &[String]) -> Result<(), String> {
    let params = args.join(" ");
    let exe_h = windows::core::HSTRING::from(exe);
    let dir_h = windows::core::HSTRING::from(work_dir);
    let params_h = windows::core::HSTRING::from(params);
    let verb = windows::core::HSTRING::from("open");
    let result = unsafe {
        ShellExecuteW(
            None,
            &verb,
            &exe_h,
            &params_h,
            &dir_h,
            SW_SHOW,
        )
    };
    if result.0 as isize <= 32 {
        return Err(format!("launching {exe} failed (ShellExecuteW={:?})", result.0));
    }
    Ok(())
}

pub fn uninstall_files(pack: &AppPack, dir: &str) {
    kill_processes(&pack.processes, 4);

    if let Ok(programs) = start_menu_dir() {
        delete_if_exists(&join_dir(&programs, &format!("{}.lnk", pack.start_menu_name)));
        delete_if_exists(&join_dir(
            &programs,
            &format!("Uninstall {}.lnk", pack.start_menu_name),
        ));
    }
    if let Ok(desktop) = desktop_dir() {
        delete_if_exists(&join_dir(&desktop, &format!("{}.lnk", pack.start_menu_name)));
    }
    if let Some(parent) = std::path::Path::new(dir).parent() {
        let company = parent
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        if company == pack.publisher {
            if let Ok(programs) = start_menu_dir() {
                delete_if_exists(&join_dir(&programs, &company));
            }
        }
    }
    std::thread::sleep(Duration::from_millis(250));
    delete_if_exists(dir);
}

/// The respawned temp-copy uninstaller removes itself after a successful
/// run: a running image cannot be deleted, but it CAN be renamed, so the
/// file is moved aside and a detached `cmd` sweep deletes it once this
/// process has exited. (The ARP copy inside the install dir needs none of
/// this — the directory delete takes it with everything else.)
pub fn cleanup_temp_uninstaller() {
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let Some(parent) = exe.parent() else {
        return;
    };
    if parent != std::env::temp_dir().join("Deploy") {
        return;
    }
    let renamed = exe.with_file_name(format!(
        "{}.{}.old",
        exe.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "uninstall".to_string()),
        std::process::id()
    ));
    if let Err(e) = std::fs::rename(&exe, &renamed) {
        log_line(&format!("uninstaller self-rename failed: {e}"));
        return;
    }
    // The renamed image stays mapped until this process exits (the user may
    // still be looking at the final screen), so the sweep retries the delete
    // on a two-second cadence for up to two minutes. The script rides the
    // command line verbatim: normal arg quoting wraps the whole line in
    // quotes, which cmd /c then misparses and the sweep never runs.
    let target = renamed.to_string_lossy();
    let script = format!(
        "for /l %i in (1,1,60) do @(del /f /q \"{target}\" & ping -n 2 127.0.0.1 >nul)"
    );
    let _ = crate::util::spawn_hidden_raw("cmd", &script);
}
