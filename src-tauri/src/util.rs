use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

static LOG: Mutex<Option<std::path::PathBuf>> = Mutex::new(None);

pub fn log_path() -> std::path::PathBuf {
    let mut guard = LOG.lock().unwrap();
    if let Some(p) = &*guard {
        return p.clone();
    }
    let app = std::env::args()
        .next()
        .and_then(|a| {
            std::path::Path::new(&a)
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
        })
        .unwrap_or_else(|| "deploy".to_string());
    let p = std::env::temp_dir().join(format!("{app}.log"));
    *guard = Some(p.clone());
    p
}

pub fn log_line(msg: &str) {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path())
    {
        let _ = writeln!(f, "[{ts}] {msg}");
    }
}

pub fn run_hidden(program: &str, args: &[&str]) -> Result<String, String> {
    let mut cmd = Command::new(program);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let out = cmd
        .output()
        .map_err(|e| format!("{program} failed to start: {e}"))?;
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    log_line(&format!("{program} {:?} -> {} ({})", args, out.status, combined.trim()));
    if out.status.success() {
        Ok(combined)
    } else {
        Err(if combined.trim().is_empty() {
            format!("{program} exited with {}", out.status)
        } else {
            combined.trim().to_string()
        })
    }
}

/// Starts a process without waiting for it (fire and forget), hidden.
pub fn spawn_hidden(program: &str, args: &[&str]) -> Result<(), String> {
    let mut cmd = Command::new(program);
    cmd.args(args).stdin(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd.spawn()
        .map(|_| ())
        .map_err(|e| format!("{program} failed to start: {e}"))
}

/// Like [`spawn_hidden`], but `arg` is appended to the command line verbatim
/// (no quoting). Needed for cmd.exe one-liners whose quoting cmd would
/// otherwise misparse.
pub fn spawn_hidden_raw(program: &str, arg: &str) -> Result<(), String> {
    #[cfg(windows)]
    use std::os::windows::process::CommandExt;
    let mut cmd = Command::new(program);
    cmd.raw_arg("/c").raw_arg(arg).stdin(Stdio::null());
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd.spawn()
        .map(|_| ())
        .map_err(|e| format!("{program} failed to start: {e}"))
}

/// Runs a program elevated (the `runas` verb — triggers one UAC prompt) and
/// WAITS for it to exit. `args` is the full argument string; the console
/// window is hidden. Callers depend on the elevated side effect being
/// COMPLETE when this returns (machine-store certificate trust must exist
/// before package registration), so ShellExecuteExW is used with
/// SEE_MASK_NOCLOSEPROCESS and the process handle is waited on. A declined
/// prompt is a failure (the launch reports ERROR_CANCELLED / SE_ERR cancel).
pub fn run_elevated(program: &str, args: &str) -> Result<(), String> {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject, INFINITE};
    use windows::Win32::UI::Shell::{
        ShellExecuteExW, SEE_MASK_FLAG_NO_UI, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
    };
    use windows::Win32::UI::WindowsAndMessaging::SW_HIDE;

    let mut file_w: Vec<u16> = program.encode_utf16().collect();
    file_w.push(0);
    let mut params_w: Vec<u16> = args.encode_utf16().collect();
    params_w.push(0);
    let verb_w: Vec<u16> = "runas\0".encode_utf16().collect();

    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS | SEE_MASK_FLAG_NO_UI,
        lpVerb: PCWSTR(verb_w.as_ptr()),
        lpFile: PCWSTR(file_w.as_ptr()),
        lpParameters: PCWSTR(params_w.as_ptr()),
        nShow: SW_HIDE.0,
        ..Default::default()
    };
    unsafe {
        ShellExecuteExW(&mut info)
            .map_err(|e| format!("{program} could not be launched elevated: {e}"))?;
        let se = info.hInstApp.0 as isize;
        if se <= 32 {
            return Err(format!("{program} could not be run elevated (SE_ERR={se})"));
        }
        if !info.hProcess.is_invalid() {
            WaitForSingleObject(info.hProcess, INFINITE);
            let mut exit_code: u32 = 0;
            if GetExitCodeProcess(info.hProcess, &mut exit_code).is_ok() && exit_code != 0 {
                log_line(&format!("elevated {program} exited with {exit_code}"));
            }
            let _ = CloseHandle(info.hProcess);
        }
    }
    log_line(&format!("elevated {program} accepted and completed"));
    Ok(())
}
