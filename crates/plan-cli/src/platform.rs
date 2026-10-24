//! OS integration for the browser-backed desktop client. No system-wide installer.
use std::{
    env,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn nonempty(value: Option<OsString>) -> Option<PathBuf> {
    value.filter(|v| !v.is_empty()).map(PathBuf::from)
}

fn data_dir_for(
    os: &str,
    mut get: impl FnMut(&str) -> Option<OsString>,
) -> Result<PathBuf, String> {
    if let Some(path) = nonempty(get("FORGEPLAN_DATA_DIR")) {
        return if path.is_absolute() {
            Ok(path)
        } else {
            Err("FORGEPLAN_DATA_DIR must be absolute".into())
        };
    }
    let base = match os {
        "windows" => nonempty(get("LOCALAPPDATA"))
            .filter(|p| p.is_absolute())
            .map(|p| p.join("ForgePlan")),
        "macos" => nonempty(get("HOME"))
            .filter(|p| p.is_absolute())
            .map(|p| p.join("Library/Application Support/ForgePlan")),
        "linux" => nonempty(get("XDG_DATA_HOME"))
            .filter(|p| p.is_absolute())
            .map(|p| p.join("forgeplan"))
            .or_else(|| {
                nonempty(get("HOME"))
                    .filter(|p| p.is_absolute())
                    .map(|p| p.join(".local/share/forgeplan"))
            }),
        _ => None,
    };
    base.ok_or_else(|| "Cannot resolve OS data directory; set absolute FORGEPLAN_DATA_DIR or pass an explicit database path".into())
}

pub fn data_dir() -> Result<PathBuf, String> {
    data_dir_for(env::consts::OS, |key| env::var_os(key))
}

fn ensure_data_dir(path: &Path) -> Result<(), String> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder
        .create(path)
        .map_err(|e| format!("Cannot create data directory {}: {e}", path.display()))
}

pub fn build_info() -> serde_json::Value {
    serde_json::json!({"version":env!("CARGO_PKG_VERSION"),"os":env::consts::OS,"arch":env::consts::ARCH,"target":env!("FORGEPLAN_BUILD_TARGET"),"desktop_ui":if cfg!(feature="native-client"){ "native_webview" }else{ "system_browser" },"native_client":cfg!(feature="native-client")})
}

pub fn print_paths() -> Result<(), String> {
    let path = data_dir()?;
    println!(
        "{}",
        serde_json::json!({"version":env!("CARGO_PKG_VERSION"),"os":env::consts::OS,"arch":env::consts::ARCH,"data_dir":path,"desktop_database":path.join("forgeplan.sqlite"),"desktop_ui":if cfg!(feature="native-client"){ "native_webview" }else{ "system_browser" },"native_client":cfg!(feature="native-client")})
    );
    Ok(())
}

pub struct DesktopOptions {
    pub port: u16,
    pub database: String,
    pub open_browser: bool,
}
impl DesktopOptions {
    pub fn parse(args: &[String]) -> Result<Self, String> {
        let mut positionals = Vec::new();
        let mut open_browser = true;
        for arg in args {
            if arg == "--no-browser" {
                open_browser = false;
            } else if arg.starts_with('-') {
                return Err(format!("Unknown desktop option: {arg}"));
            } else {
                positionals.push(arg.as_str());
            }
        }
        if positionals.len() > 2 {
            return Err("Usage: forgeplan desktop [PORT] [DB] [--no-browser]".into());
        }
        let port = positionals
            .first()
            .map(|v| v.parse::<u16>())
            .transpose()
            .map_err(|_| "Invalid port")?
            .unwrap_or(8787);
        let database = if let Some(path) = positionals.get(1) {
            (*path).to_owned()
        } else {
            let dir = data_dir()?;
            ensure_data_dir(&dir)?;
            dir.join("forgeplan.sqlite")
                .into_os_string()
                .into_string()
                .map_err(|_| {
                    "Data path is not valid Unicode; use an explicit Unicode database path"
                })?
        };
        Ok(Self {
            port,
            database,
            open_browser,
        })
    }
}

pub(crate) fn server_url(input: &str) -> Result<String, String> {
    let url = reqwest::Url::parse(input).map_err(|_| "Invalid server URL")?;
    let local = matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"));
    if !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.host_str().is_none()
        || !(url.scheme() == "https" || (url.scheme() == "http" && local))
        || url.path() != "/"
    {
        return Err("Use a root HTTPS server URL (loopback HTTP is allowed), without credentials, query or fragment".into());
    }
    Ok(url.to_string())
}

pub fn connect(input: &str) -> Result<(), String> {
    let url = server_url(input)?;
    println!("Open {url}\nSign in in your browser. No local database is created.");
    open_browser(&url)
}

pub fn open_browser(url: &str) -> Result<(), String> {
    // Pass URL as an argument, never through cmd /c or a shell string.
    let url = server_url(url)?;
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut c = Command::new("rundll32.exe");
        c.arg("url.dll,FileProtocolHandler");
        c
    };
    #[cfg(target_os = "macos")]
    let mut command = Command::new("/usr/bin/open");
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let mut command = Command::new("xdg-open");
    let status = command.arg(url).status().map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("System browser handler returned {status}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn resolve(os: &str, vars: &[(&str, &str)]) -> Result<PathBuf, String> {
        data_dir_for(os, |key| {
            vars.iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| OsString::from(v))
        })
    }
    #[test]
    fn paths_follow_platform_conventions_without_cwd_fallback() {
        #[cfg(unix)]
        {
            assert_eq!(
                resolve("macos", &[("HOME", "/Users/Max")]).unwrap(),
                PathBuf::from("/Users/Max/Library/Application Support/ForgePlan")
            );
            assert_eq!(
                resolve("linux", &[("HOME", "/home/max")]).unwrap(),
                PathBuf::from("/home/max/.local/share/forgeplan")
            );
            assert_eq!(
                resolve(
                    "linux",
                    &[("HOME", "/home/max"), ("XDG_DATA_HOME", "relative")]
                )
                .unwrap(),
                PathBuf::from("/home/max/.local/share/forgeplan")
            );
            assert_eq!(
                resolve("windows", &[("LOCALAPPDATA", "/win/profile")]).unwrap(),
                PathBuf::from("/win/profile/ForgePlan")
            );
            assert_eq!(
                resolve("linux", &[("XDG_DATA_HOME", "/data")]).unwrap(),
                PathBuf::from("/data/forgeplan")
            );
            assert_eq!(
                resolve("macos", &[("FORGEPLAN_DATA_DIR", "/portable/data")]).unwrap(),
                PathBuf::from("/portable/data")
            );
        }
        #[cfg(windows)]
        {
            assert_eq!(
                resolve(
                    "windows",
                    &[("LOCALAPPDATA", "C:\\Users\\Max\\AppData\\Local")]
                )
                .unwrap(),
                PathBuf::from("C:\\Users\\Max\\AppData\\Local\\ForgePlan")
            );
        }
        assert!(resolve("linux", &[]).is_err());
        assert!(resolve("macos", &[("FORGEPLAN_DATA_DIR", "relative")]).is_err());
    }
    #[test]
    fn remote_client_rejects_unsafe_urls() {
        for input in [
            "file:///etc/passwd",
            "javascript:alert(1)",
            "http://projects.example",
            "https://u:p@projects.example/",
            "https://projects.example/?token=secret",
            "https://projects.example/#token",
            "https://projects.example/app",
            "https://projects.example/%0a",
        ] {
            assert!(server_url(input).is_err(), "{input}");
        }
        assert_eq!(
            server_url("https://projects.example").unwrap(),
            "https://projects.example/"
        );
        assert_eq!(
            server_url("http://127.0.0.1:8787").unwrap(),
            "http://127.0.0.1:8787/"
        );
        assert!(server_url("http://[::1]:8787").is_ok());
    }
    #[test]
    fn explicit_database_path_is_preserved_and_headless_is_supported() {
        let options = DesktopOptions::parse(&[
            "0".into(),
            "folder with spaces/план.sqlite".into(),
            "--no-browser".into(),
        ])
        .unwrap();
        assert_eq!(options.port, 0);
        assert!(!options.open_browser);
        assert_eq!(options.database, "folder with spaces/план.sqlite");
        assert!(DesktopOptions::parse(&["invalid".into()]).is_err());
        assert!(DesktopOptions::parse(&["--unknown".into()]).is_err());
    }
}
