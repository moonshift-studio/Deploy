use serde::Deserialize;

#[derive(Deserialize, Clone)]
pub struct AppPack {
    #[serde(rename = "appName")]
    pub app_name: String,
    pub description: String,
    pub publisher: String,
    #[serde(rename = "identityName")]
    pub identity_name: String,
    #[serde(rename = "identityPublisher")]
    pub identity_publisher: String,
    #[serde(rename = "defaultDir")]
    pub default_dir: String,
    #[serde(rename = "registryKey")]
    pub registry_key: String,
    #[serde(rename = "launchExe")]
    pub launch_exe: String,
    #[serde(rename = "startMenuName")]
    pub start_menu_name: String,
    #[serde(rename = "autostartName")]
    pub autostart_name: String,
    pub processes: Vec<String>,
}

impl AppPack {
    pub fn embedded() -> AppPack {
        serde_json::from_str(crate::assets::APPPACK_JSON).expect("embedded app pack must be valid")
    }

    pub fn resolved_default_dir(&self) -> String {
        expand(&self.default_dir)
    }

    pub fn installed_exe(&self, dir: &str) -> String {
        join_dir(dir, &self.launch_exe)
    }
}

pub fn join_dir(dir: &str, name: &str) -> String {
    if dir.ends_with('\\') {
        format!("{dir}{name}")
    } else {
        format!("{dir}\\{name}")
    }
}

pub fn expand(template: &str) -> String {
    let mut out = template.to_string();
    for key in ["LOCALAPPDATA", "APPDATA", "USERPROFILE", "PROGRAMDATA"] {
        if let Ok(val) = std::env::var(key) {
            out = out.replace(&format!("%{key}%"), &val);
        }
    }
    out
}
