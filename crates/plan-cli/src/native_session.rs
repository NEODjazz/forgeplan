//! Owned local process: random per-launch API token, readiness and cleanup.
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::mpsc,
    time::Duration,
};

fn application_setting(key: &std::ffi::OsStr) -> bool {
    key.to_string_lossy()
        .to_ascii_uppercase()
        .starts_with("FORGEPLAN_")
}

pub struct LocalSession {
    child: Child,
    pub url: String,
    pub token: String,
}
impl LocalSession {
    pub fn start(database: &Path) -> Result<Self, String> {
        let mut random = [0u8; 32];
        getrandom::getrandom(&mut random).map_err(|e| e.to_string())?;
        let token: String = random.iter().map(|b| format!("{b:02x}")).collect();
        let mut command = Command::new(std::env::current_exe().map_err(|e| e.to_string())?);
        command
            .args(["workspace", "0"])
            .arg(database)
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        for (key, _) in std::env::vars_os() {
            if application_setting(&key) {
                command.env_remove(key);
            }
        }
        command.env("FORGEPLAN_IDENTITIES", serde_json::json!({token.clone():{"subject":"desktop","organization":"local","role":"admin"}}).to_string());
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let mut child = command.spawn().map_err(|e| e.to_string())?;
        let stdout = child
            .stdout
            .take()
            .ok_or("Cannot read local process readiness")?;
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            let mut line = String::new();
            let result = reader
                .read_line(&mut line)
                .map_err(|e| e.to_string())
                .and_then(|_| ready_url(&line));
            let _ = sender.send(result);
            // Drain diagnostics without recording the ephemeral token.
            for line in reader.lines() {
                if line.is_err() {
                    break;
                }
            }
        });
        let result = receiver
            .recv_timeout(Duration::from_secs(10))
            .map_err(|_| "Local process did not become ready within 10 seconds".to_string())
            .and_then(|r| r);
        match result {
            Ok(url) => Ok(Self { child, url, token }),
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                Err(error)
            }
        }
    }
}
impl Drop for LocalSession {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
fn ready_url(line: &str) -> Result<String, String> {
    let url = line
        .trim()
        .strip_prefix("Open ")
        .ok_or("Invalid local readiness response")?;
    let parsed = reqwest::Url::parse(url).map_err(|_| "Invalid local readiness URL")?;
    if parsed.scheme() != "http"
        || parsed.host_str() != Some("127.0.0.1")
        || parsed.port().is_none()
        || parsed.path() != "/"
        || parsed.query().is_some()
        || parsed.fragment().is_some()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return Err("Local process must report an exact loopback URL".into());
    }
    Ok(parsed.to_string())
}
#[derive(Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Preferences {
    pub server: String,
}
impl Preferences {
    pub fn read(dir: &Path) -> Result<Self, String> {
        let file = dir.join("client-settings.json");
        if !file.exists() {
            return Ok(Self::default());
        }
        let data = fs::read_to_string(file).map_err(|e| e.to_string())?;
        if data.len() > 8192 {
            return Err("Client settings exceed 8 KB".into());
        }
        let result: Self = serde_json::from_str(&data).map_err(|e| e.to_string())?;
        if !result.server.is_empty() {
            crate::platform::server_url(&result.server)?;
        }
        Ok(result)
    }
    pub fn write(&self, dir: &Path) -> Result<(), String> {
        crate::platform::server_url(&self.server)?;
        let file = dir.join("client-settings.json");
        let mut random = [0u8; 16];
        getrandom::getrandom(&mut random).map_err(|e| e.to_string())?;
        let suffix: String = random.iter().map(|b| format!("{b:02x}")).collect();
        let temporary: PathBuf = dir.join(format!(".client-settings.{suffix}.tmp"));
        use std::io::Write;
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut output = options.open(&temporary).map_err(|e| e.to_string())?;
        let result = (|| {
            output
                .write_all(
                    serde_json::to_string_pretty(self)
                        .map_err(|e| e.to_string())?
                        .as_bytes(),
                )
                .map_err(|e| e.to_string())?;
            output.sync_all().map_err(|e| e.to_string())?;
            drop(output);
            fs::rename(&temporary, file).map_err(|e| e.to_string())
        })();
        if result.is_err() {
            let _ = fs::remove_file(temporary);
        }
        result
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inherited_application_settings_are_removed_case_insensitively() {
        for key in [
            "FORGEPLAN_BIND",
            "forgeplan_oidc_issuer",
            "ForgePlan_Public_Origin",
        ] {
            assert!(application_setting(std::ffi::OsStr::new(key)));
        }
        for key in ["PATH", "RUST_LOG", "NOT_FORGEPLAN_BIND"] {
            assert!(!application_setting(std::ffi::OsStr::new(key)));
        }
    }
    #[test]
    fn preferences_recover_after_stale_temp_and_replace_without_storing_secrets() {
        let mut bytes = [0u8; 16];
        getrandom::getrandom(&mut bytes).unwrap();
        let name: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
        let dir = std::env::temp_dir().join(format!("forgeplan-settings-{name}"));
        fs::create_dir(&dir).unwrap();
        let stale = dir.join("client-settings.json.tmp");
        fs::write(&stale, "interrupted previous save").unwrap();
        Preferences {
            server: "https://one.example/".into(),
        }
        .write(&dir)
        .unwrap();
        assert_eq!(
            Preferences::read(&dir).unwrap().server,
            "https://one.example/"
        );
        Preferences {
            server: "https://two.example/".into(),
        }
        .write(&dir)
        .unwrap();
        assert_eq!(
            Preferences::read(&dir).unwrap().server,
            "https://two.example/"
        );
        assert!(Preferences {
            server: "https://user:secret@host.example/".into()
        }
        .write(&dir)
        .is_err());
        assert_eq!(
            Preferences::read(&dir).unwrap().server,
            "https://two.example/"
        );
        assert_eq!(
            fs::read_to_string(&stale).unwrap(),
            "interrupted previous save"
        );
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 2);
        fs::remove_file(dir.join("client-settings.json")).unwrap();
        fs::remove_file(stale).unwrap();
        fs::remove_dir(dir).unwrap();
    }
    #[test]
    fn readiness_rejects_nonlocal_or_ambiguous_locations() {
        assert_eq!(
            ready_url("Open http://127.0.0.1:4587\n").unwrap(),
            "http://127.0.0.1:4587/"
        );
        for line in [
            "Open https://projects.example.org",
            "Open http://127.0.0.1",
            "Open http://127.0.0.1:1/?secret=x",
            "Open http://127.0.0.1:1/app",
            "Unexpected",
        ] {
            assert!(ready_url(line).is_err());
        }
    }
}
