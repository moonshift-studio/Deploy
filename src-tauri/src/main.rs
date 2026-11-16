#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let invocation = deploy_lib::cli::parse();
    // The uninstaller copy registered with Windows lives inside the install
    // directory; hand the work over to a temp copy so the directory can go.
    if deploy_lib::respawn_installed_uninstaller(&invocation.mode) {
        return;
    }
    if invocation.silent {
        std::process::exit(deploy_lib::headless(invocation.mode));
    }
    deploy_lib::run_ui(invocation.mode);
}
