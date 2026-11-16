use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::apppack::AppPack;
use crate::assets::{APP_VERSION, IDENTITY_MSIX, ICON_PNG, PAYLOAD_FILES};
use crate::engine::{self, Progress};

#[derive(Serialize, Clone)]
pub struct AppInfo {
    pub mode: String,
    pub app_name: String,
    pub version: String,
    pub description: String,
    pub publisher: String,
    pub icon: Option<String>,
    pub default_dir: String,
    pub existing_dir: Option<String>,
    pub has_identity_package: bool,
    pub payload_files: usize,
    pub restart: bool,
    pub app_args: Vec<String>,
}

#[tauri::command]
pub fn app_info() -> AppInfo {
    let pack = AppPack::embedded();
    let mode = crate::cli::ui_mode();
    let (restart, app_args) = match mode {
        crate::cli::Mode::Update { restart, app_args, .. } => (*restart, app_args.clone()),
        _ => (false, Vec::new()),
    };
    AppInfo {
        mode: mode.as_str().to_string(),
        app_name: pack.app_name.clone(),
        version: APP_VERSION.to_string(),
        description: pack.description.clone(),
        publisher: pack.publisher.clone(),
        icon: ICON_PNG
            .filter(|b| !b.is_empty())
            .map(|b| format!("data:image/png;base64,{}", STANDARD.encode(b))),
        default_dir: pack.resolved_default_dir(),
        existing_dir: crate::registry::get_install_dir(&pack),
        has_identity_package: IDENTITY_MSIX.map_or(false, |b| !b.is_empty()),
        payload_files: PAYLOAD_FILES.len(),
        restart,
        app_args,
    }
}

#[tauri::command]
pub fn choose_dir(current: String) -> Option<String> {
    rfd::FileDialog::new()
        .set_title("Choose installation folder")
        .set_directory(&current)
        .pick_folder()
        .map(|p| p.to_string_lossy().to_string())
}

fn emit(app: &AppHandle, p: Progress) {
    let _ = app.emit("deploy://progress", p);
}

fn report_error(app: &AppHandle, e: String) {
    emit(
        app,
        Progress {
            stage: "done".into(),
            detail: e.clone(),
            percent: 100,
            done: true,
            error: Some(e),
        },
    );
}

#[tauri::command]
pub async fn begin_install(
    app: AppHandle,
    dir: String,
    start_menu: bool,
    desktop: bool,
    autostart: bool,
    launch_after: bool,
) -> Result<(), String> {
    let pack = AppPack::embedded();
    let opts = engine::InstallOptions {
        dir,
        start_menu,
        desktop,
        autostart,
        launch_after,
    };
    std::thread::spawn(move || {
        let mut sink = |p: Progress| emit(&app, p);
        if let Err(e) = engine::install(&pack, &opts, &mut sink) {
            report_error(&app, e);
        }
    });
    Ok(())
}

#[tauri::command]
pub async fn begin_update(
    app: AppHandle,
    restart: bool,
    app_args: Vec<String>,
) -> Result<(), String> {
    let pack = AppPack::embedded();
    std::thread::spawn(move || {
        let mut sink = |p: Progress| emit(&app, p);
        if let Err(e) = engine::update(&pack, None, &app_args, restart, &mut sink) {
            report_error(&app, e);
        }
    });
    Ok(())
}

#[tauri::command]
pub async fn begin_uninstall(app: AppHandle) -> Result<(), String> {
    let pack = AppPack::embedded();
    std::thread::spawn(move || {
        let mut sink = |p: Progress| emit(&app, p);
        if let Err(e) = engine::uninstall(&pack, &mut sink) {
            report_error(&app, e);
        }
    });
    Ok(())
}
