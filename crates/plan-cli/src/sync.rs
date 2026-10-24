//! Offline JSON exchange with the loopback workspace. Tokens stay in the environment.
use plan_core::*;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{Read, Write},
    net::TcpStream,
    path::Path,
};
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Link {
    url: String,
    organization: String,
    project: String,
    revision: u64,
}
fn request(link: &Link, path: &str, body: Option<&str>) -> Result<String> {
    let host = link
        .url
        .strip_prefix("http://127.0.0.1:")
        .filter(|s| s.bytes().all(|c| c.is_ascii_digit()))
        .ok_or("Offline CLI currently supports http://127.0.0.1:PORT only")?;
    let port: u16 = host.parse().map_err(|_| "Invalid port")?;
    let host = format!("127.0.0.1:{port}");
    let mut stream = TcpStream::connect(&host).map_err(|e| e.to_string())?;
    let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(10)));
    let token = std::env::var("FORGEPLAN_TOKEN").ok();
    if token.as_ref().is_some_and(|t| t.contains(['\r', '\n']))
        || [&link.organization, &link.project]
            .iter()
            .any(|v| v.contains(['\r', '\n']))
    {
        return Err("Invalid sync headers".into());
    }
    let auth = token
        .map(|t| format!("Authorization: Bearer {t}\r\n"))
        .unwrap_or_default();
    let key = body
        .map(|b| {
            use sha2::{Digest, Sha256};
            format!("{:x}", Sha256::digest(b.as_bytes()))
        })
        .unwrap_or_default();
    let b = body.unwrap_or("");
    write!(stream,"{} {path} HTTP/1.1\r\nHost: {host}\r\nOrigin: {}\r\nX-Organization: {}\r\nX-Project-ID: {}\r\nX-Command-ID: {key}\r\n{auth}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{b}",if body.is_some(){"POST"}else{"GET"},link.url,link.organization,link.project,b.len()).map_err(|e|e.to_string())?;
    let mut result = String::new();
    stream
        .take(16_000_001)
        .read_to_string(&mut result)
        .map_err(|e| e.to_string())?;
    if result.len() > 16_000_000 {
        return Err("Workspace response exceeds 16 MB".into());
    }
    let (head, body) = result
        .split_once("\r\n\r\n")
        .ok_or("Invalid HTTP response")?;
    if !head.starts_with("HTTP/1.1 200") {
        return Err(format!("Sync failed: {body}"));
    }
    Ok(body.into())
}
pub fn pull(url: &str, org: &str, project: &str, path: &str) -> Result<()> {
    let metadata = Path::new(path).with_extension("sync.json");
    if Path::new(path).exists() || metadata.exists() {
        return Err("Pull destination already exists".into());
    }
    let mut link = Link {
        url: url.trim_end_matches('/').into(),
        organization: org.into(),
        project: project.into(),
        revision: 0,
    };
    let state: serde_json::Value =
        serde_json::from_str(&request(&link, "/api/state", None)?).map_err(|e| e.to_string())?;
    let document = state["document"].as_str().ok_or("Missing document")?;
    parse(document)?;
    link.revision = state["revision"].as_u64().ok_or("Missing revision")?;
    fs::write(path, document).map_err(|e| e.to_string())?;
    if let Err(e) = fs::write(
        metadata,
        serde_json::to_string_pretty(&link).map_err(|e| e.to_string())?,
    ) {
        let _ = fs::remove_file(path);
        return Err(e.to_string());
    }
    Ok(())
}
pub fn push(path: &str) -> Result<()> {
    let metadata = Path::new(path).with_extension("sync.json");
    let mut link: Link =
        serde_json::from_str(&fs::read_to_string(&metadata).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    let p = parse(&fs::read_to_string(path).map_err(|e| e.to_string())?)?;
    let body=serde_json::json!({"base_revision":link.revision,"commands":[Command::Replace(Box::new(p))]}).to_string();
    let state: serde_json::Value =
        serde_json::from_str(&request(&link, "/api/commands", Some(&body))?)
            .map_err(|e| e.to_string())?;
    link.revision = state["revision"].as_u64().ok_or("Missing revision")?;
    fs::write(
        metadata,
        serde_json::to_string_pretty(&link).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}
