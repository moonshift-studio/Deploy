use std::{
    env, fs,
    path::{Path, PathBuf},
};

/// deploy embeds everything an app needs at build time. The consuming app
/// (which depends on deploy as its installer) points DEPLOY_APP_CONFIG at its
/// own `app-pack.json`; all paths inside it resolve relative to that file.
/// There are no defaults: without a config, deploy has nothing to install.
fn main() {
    tauri_build::build();

    let config_path = env::var("DEPLOY_APP_CONFIG").unwrap_or_else(|_| {
        // Dev fallback: without a config, use the local dev pack so
        // `tauri dev` works out of the box for UI testing.
        let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("dev-app-pack.json");
        println!(
            "cargo:warning=deploy: DEPLOY_APP_CONFIG not set; using dev app pack {}",
            dev.display()
        );
        dev.to_string_lossy().into_owned()
    });
    let config_path = PathBuf::from(&config_path);
    println!("cargo:rerun-if-env-changed=DEPLOY_APP_CONFIG");
    println!("cargo:rerun-if-changed={}", config_path.display());
    let base = config_path.parent().unwrap().to_path_buf();

    let cfg: serde_json::Value = match fs::read_to_string(&config_path) {
        Ok(text) => serde_json::from_str(&text)
            .unwrap_or_else(|e| panic!("{} is not valid JSON: {e}", config_path.display())),
        Err(e) => panic!("could not read {}: {e}", config_path.display()),
    };

    let version_source = cfg["versionSource"].as_str().unwrap_or("");
    let version = if version_source.is_empty() {
        "0.0.0".to_string()
    } else {
        let p = base.join(version_source);
        println!("cargo:rerun-if-changed={}", p.display());
        let text = fs::read_to_string(&p).unwrap_or_else(|e| panic!("versionSource missing: {e}"));
        serde_json::from_str::<serde_json::Value>(&text)
            .ok()
            .and_then(|v| v["version"].as_str().map(str::to_string))
            .unwrap_or_else(|| "0.0.0".to_string())
    };

    let mut entries = Vec::new();
    if let Some(payload) = cfg["payload"].as_array() {
        for item in payload {
            let src = base.join(item["path"].as_str().unwrap_or_default());
            let name = item["name"].as_str().unwrap_or_default().to_string();
            println!("cargo:rerun-if-changed={}", src.display());
            match fs::metadata(&src) {
                Ok(_) => entries.push(format!(
                    "    PayloadFile {{ name: {name:?}, bytes: include_bytes!({src:?}) }},",
                    name = name,
                    src = slash(&src)
                )),
                Err(_) => panic!(
                    "deploy: payload file missing: {} (build the app first)",
                    src.display()
                ),
            }
        }
    }

    let embed = |key: &str| -> (bool, String) {
        match cfg[key].as_str().filter(|s| !s.is_empty()) {
            Some(rel) => {
                let p = base.join(rel);
                println!("cargo:rerun-if-changed={}", p.display());
                if p.is_file() {
                    (true, slash(&p))
                } else {
                    println!(
                        "cargo:warning=deploy: {} not found at {} (skipped)",
                        key,
                        p.display()
                    );
                    (false, String::new())
                }
            }
            None => (false, String::new()),
        }
    };
    let (has_icon, icon_path) = embed("iconPath");
    let (has_cert, cert_path) = embed("certPath");
    let (has_msix, msix_path) = embed("msixPath");

    let out = env::var("OUT_DIR").unwrap();
    let mut code = String::new();
    code.push_str(&format!(
        "pub static APPPACK_JSON: &str = r##\"{}\"##;\n",
        cfg.to_string().replace("\"##", "\\\"##")
    ));
    code.push_str(&format!("pub static APP_VERSION: &str = {version:?};\n"));
    code.push_str("pub struct PayloadFile { pub name: &'static str, pub bytes: &'static [u8] }\n");
    code.push_str("pub static PAYLOAD_FILES: &[PayloadFile] = &[\n");
    for e in &entries {
        code.push_str(e);
    }
    code.push_str("];\n");
    code.push_str(&format!(
        "pub static ICON_PNG: Option<&[u8]> = {};\n",
        if has_icon {
            format!("Some(include_bytes!({icon_path:?}))")
        } else {
            "None".to_string()
        }
    ));
    code.push_str(&format!(
        "pub static CERT_CER: Option<&[u8]> = {};\n",
        if has_cert {
            format!("Some(include_bytes!({cert_path:?}))")
        } else {
            "None".to_string()
        }
    ));
    code.push_str(&format!(
        "pub static IDENTITY_MSIX: Option<&[u8]> = {};\n",
        if has_msix {
            format!("Some(include_bytes!({msix_path:?}))")
        } else {
            "None".to_string()
        }
    ));
    fs::write(Path::new(&out).join("deploy_assets.rs"), code).unwrap();
}

fn slash(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}
