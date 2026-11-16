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

/// Runs a program elevated (the `runas` verb — triggers one UAC prompt).
/// `args` is the full argument string; the console window is hidden. The
/// user declining the prompt is a failure (SE_ERR_CANCELLED).
pub fn run_elevated(program: &str, args: &str) -> Result<(), String> {
    use windows::core::HSTRING;
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_HIDE;

    let verb = HSTRING::from("runas");
    let exe = HSTRING::from(program);
    let params = HSTRING::from(args);
    let result = unsafe { ShellExecuteW(None, &verb, &exe, &params, None, SW_HIDE) };
    let code = result.0 as isize;
    if code > 32 && code != 1223 {
        log_line(&format!("elevated {program} accepted"));
        Ok(())
    } else {
        Err(format!(
            "{program} could not be run elevated (ShellExecuteW={code})"
        ))
    }
}
