pub mod assets {
    include!(concat!(env!("OUT_DIR"), "/deploy_assets.rs"));
}
pub mod apppack;
pub mod cert;
pub mod cli;
pub mod commands;
pub mod engine;
pub mod files;
pub mod identity;
pub mod registry;
pub mod util;

use apppack::AppPack;
use cli::Mode;
use glass_ui::{commands as glass_commands, MenuGlassManager};
use liquid_glass::{configure_borderless_host, install_activation_subclass};
use tauri::{AppHandle, Manager, WebviewWindow};
use util::log_line;
use windows::Win32::Foundation::HWND;

pub fn run_ui(mode: Mode) {
    cli::set_ui_mode(mode);
    let pill = matches!(cli::ui_mode(), Mode::Update { passive: true, .. });

    if !acquire_single_instance() {
        return;
    }

    let pack = AppPack::embedded();
    tauri::Builder::default()
        .manage(MenuGlassManager::new())
        .setup(move |app| {
            let mut builder =
                tauri::WebviewWindowBuilder::new(app, "main", tauri::WebviewUrl::default())
                    .title(&pack.app_name)
                    .decorations(false)
                    .transparent(true)
                    .resizable(false)
                    .shadow(false);
            builder = if pill {
                builder
                    .inner_size(400.0, 180.0)
                    .skip_taskbar(true)
                    .always_on_top(true)
            } else {
                builder.inner_size(1280.0, 300.0).center()
            };
            let window = builder.build()?;
            prepare_glass_host(&window);
            track_glass_bounds(&window, app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::choose_dir,
            commands::begin_install,
            commands::begin_update,
            commands::begin_uninstall,
            glass_commands::position_menu_glass,
            glass_commands::animate_menu_glass,
            glass_commands::sync_menu_glasses,
            glass_commands::prewarm_menu_glass,
            glass_commands::set_ui_hit_rects,
            glass_commands::log_message,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run deploy");
}

/// Prepares the host window for LOCAL glass surfaces (the switch thumb's
/// glass refracts the window's own pixels, so it must NOT be excluded from
/// capture — its own pixels are the backdrop).
///
/// Note: no WS_EX_LAYERED here. tao already gives transparent windows
/// WS_EX_NOREDIRECTIONBITMAP (DirectComposition), and layering that window
/// again leaves the layered path without a redirection bitmap — after every
/// DOM reflow DWM composites stale/blank regions over the fresh visual tree
/// (observed: the right half of the window going transparent on the done
/// screen, hiding the close arrow). The borderless styling below is enough;
/// the un-layered transparent webview repaints cleanly.
fn prepare_glass_host(window: &WebviewWindow) {
    let Ok(hwnd) = window.hwnd() else {
        return;
    };
    let hwnd = HWND(hwnd.0 as *mut _);
    let _ = configure_borderless_host(hwnd.0 as isize);
    let _ = install_activation_subclass(hwnd.0 as isize);
}

/// Re-syncs the window's glass surfaces whenever it moves, resizes, or is
/// activated (activation raises the host above its own glass surfaces in
/// the z-order; only a re-sync pins the glass back above it).
fn track_glass_bounds(window: &WebviewWindow, app_handle: AppHandle) {
    let tracked = window.clone();
    window.on_window_event(move |event| {
        if !matches!(
            event,
            tauri::WindowEvent::Moved(_)
                | tauri::WindowEvent::Resized(_)
                | tauri::WindowEvent::Focused(true)
        ) {
            return;
        }
        let Ok(hwnd) = tracked.hwnd() else {
            return;
        };
        app_handle
            .state::<MenuGlassManager>()
            .sync_bounds(HWND(hwnd.0 as *mut _));
    });
}

fn acquire_single_instance() -> bool {
    use windows::core::HSTRING;
    use windows::Win32::System::Threading::CreateMutexW;

    let name = HSTRING::from("Deploy.Installer.Mutex");
    match unsafe { CreateMutexW(None, false, &name) } {
        Err(_) => false,
        Ok(_) => true,
    }
}

/// When Windows launches the uninstaller copy that lives INSIDE the install
/// directory (Add/Remove Programs, the Start menu shortcut), that copy
/// cannot delete its own directory — its image file keeps the folder
/// pinned. The fix is the classic self-replacement dance: copy this exe to
/// a temp location, spawn it there with the same arguments, and exit so the
/// temp copy (outside the directory) can remove everything. Returns true
/// when a respawn took over (the caller must exit immediately).
pub fn respawn_installed_uninstaller(mode: &Mode) -> bool {
    if !matches!(mode, Mode::Uninstall { .. }) {
        return false;
    }
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    let pack = AppPack::embedded();
    let Some(dir) = registry::get_install_dir(&pack) else {
        return false;
    };
    let in_dir = match (std::fs::canonicalize(&exe), std::fs::canonicalize(&dir)) {
        (Ok(exe_path), Ok(dir_path)) => exe_path
            .parent()
            .map(|parent| parent == dir_path)
            .unwrap_or(false),
        _ => false,
    };
    if !in_dir {
        return false;
    }
    let temp_dir = std::env::temp_dir().join("Deploy");
    if std::fs::create_dir_all(&temp_dir).is_err() {
        return false;
    }
    let temp_exe = temp_dir.join(format!("{}-uninstall.exe", pack.app_name));
    if std::fs::copy(&exe, &temp_exe).is_err() {
        log_line("could not copy the uninstaller out of the install directory");
        return false;
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    // The child must not inherit the install dir as its working directory —
    // a process CWD pins the directory and the final rmdir would leave an
    // empty husk behind.
    match std::process::Command::new(&temp_exe)
        .args(&args)
        .current_dir(&temp_dir)
        .spawn()
    {
        Ok(_) => {
            log_line(&format!(
                "respawned the uninstaller from {}",
                temp_exe.display()
            ));
            true
        }
        Err(e) => {
            log_line(&format!("uninstaller respawn failed: {e}"));
            false
        }
    }
}

pub fn headless(mode: Mode) -> i32 {
    let pack = AppPack::embedded();
    let mut sink = |p: engine::Progress| {
        util::log_line(&format!(
            "{} {} ({}%) error={:?}",
            p.stage, p.detail, p.percent, p.error
        ));
    };
    let result = match mode {
        Mode::Update {
            app_args, restart, ..
        } => engine::update(&pack, None, &app_args, restart, &mut sink),
        Mode::Uninstall { .. } => engine::uninstall(&pack, &mut sink),
        Mode::Install { dir } => {
            let opts = engine::InstallOptions {
                dir: dir.unwrap_or_else(|| pack.resolved_default_dir()),
                start_menu: true,
                desktop: false,
                autostart: true,
                launch_after: false,
            };
            engine::install(&pack, &opts, &mut sink)
        }
    };
    match result {
        Ok(()) => 0,
        Err(e) => {
            util::log_line(&format!("headless run failed: {e}"));
            1
        }
    }
}
