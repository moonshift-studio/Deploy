#[derive(Debug, Clone, PartialEq)]
pub enum Mode {
    Install {
        dir: Option<String>,
    },
    Update {
        passive: bool,
        restart: bool,
        app_args: Vec<String>,
    },
    Uninstall {
        silent: bool,
    },
}

impl Mode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Mode::Install { .. } => "install",
            Mode::Update { .. } => "update",
            Mode::Uninstall { .. } => "uninstall",
        }
    }
}

static UI_MODE: std::sync::OnceLock<Mode> = std::sync::OnceLock::new();

pub fn set_ui_mode(mode: Mode) {
    let _ = UI_MODE.set(mode);
}

pub fn ui_mode() -> &'static Mode {
    UI_MODE.get_or_init(|| Mode::Install { dir: None })
}

pub struct Invocation {
    pub mode: Mode,
    pub silent: bool,
}

pub fn parse() -> Invocation {
    let args: Vec<String> = std::env::args().skip(1).collect();
    parse_args(&args)
}

pub fn parse_args(args: &[String]) -> Invocation {
    let mut dir: Option<String> = None;
    let mut passive = false;
    let mut silent = false;
    let mut update = false;
    let mut restart = false;
    let mut uninstall = false;
    let mut app_args: Vec<String> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        let lower = a.to_ascii_lowercase();
        if lower.starts_with("/d=") || lower.starts_with("-d=") {
            dir = Some(a[3..].to_string());
        } else if lower == "/d" || lower == "-d" {
            if let Some(next) = args.get(i + 1) {
                dir = Some(next.clone());
                i += 1;
            }
        } else if lower.starts_with("/args") || lower.starts_with("-args") {
            for rest in &args[i + 1..] {
                app_args.push(unquote(rest));
            }
            break;
        } else if lower.starts_with("/args=") || lower.starts_with("-args=") {
            app_args.push(unquote(&a[6..]));
        } else {
            match lower.as_str() {
                "/p" | "-p" => passive = true,
                "/s" | "-s" | "/silent" | "/q" | "/quiet" => silent = true,
                "/update" | "-update" => update = true,
                "/r" | "-r" | "/restart" => restart = true,
                "/uninstall" | "-uninstall" | "/u" | "/x" | "/remove" => uninstall = true,
                _ => {}
            }
        }
        i += 1;
    }

    let mode = if update {
        Mode::Update { passive, restart, app_args }
    } else if uninstall {
        Mode::Uninstall { silent }
    } else {
        Mode::Install { dir }
    };
    Invocation { mode, silent }
}

fn unquote(s: &str) -> String {
    let t = s.trim();
    if t.len() >= 2 && t.starts_with('"') && t.ends_with('"') {
        t[1..t.len() - 1].to_string()
    } else {
        t.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(args: &[&str]) -> Vec<String> {
        args.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn update_passive() {
        let inv = parse_args(&v(&["/P", "/UPDATE", "/R", "/ARGS", "C:\\app", "--flag"]));
        assert_eq!(
            inv.mode,
            Mode::Update {
                passive: true,
                restart: true,
                app_args: vec!["C:\\app".into(), "--flag".into()]
            }
        );
    }

    #[test]
    fn silent_uninstall() {
        let inv = parse_args(&v(&["/S", "/uninstall"]));
        assert_eq!(inv.mode, Mode::Uninstall { silent: true });
        assert!(inv.silent);
    }

    #[test]
    fn install_with_dir() {
        let inv = parse_args(&v(&["/D=C:\\Program Files\\App"]));
        assert_eq!(
            inv.mode,
            Mode::Install {
                dir: Some("C:\\Program Files\\App".into())
            }
        );
    }
}
