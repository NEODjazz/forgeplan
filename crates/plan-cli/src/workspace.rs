//! SQLite workspace with organization-scoped roles and optional OIDC access tokens.
use super::*;
use crate::store::Store;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::Digest;
use std::collections::BTreeMap;
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Role {
    Viewer,
    Editor,
    Manager,
    Admin,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Identity {
    subject: String,
    organization: String,
    role: Role,
}
fn identity(req: &Request) -> Result<Identity, String> {
    let org = header(req, "x-organization").unwrap_or("local");
    let token = header(req, "authorization").and_then(|v| v.strip_prefix("Bearer "));
    let config = env::var("FORGEPLAN_IDENTITIES").ok();
    if let (Some(config), Some(token)) = (&config, token) {
        let keys: BTreeMap<String, Identity> =
            serde_json::from_str(config).map_err(|_| "Invalid identity configuration")?;
        if let Some(value) = keys
            .iter()
            .find(|(key, _)| {
                if key.len() < 32 {
                    return false;
                }
                let a = sha2::Sha256::digest(key.as_bytes());
                let b = sha2::Sha256::digest(token.as_bytes());
                a.iter().zip(b).fold(0u8, |v, (a, b)| v | (*a ^ b)) == 0
            })
            .map(|(_, v)| v.clone())
        {
            if value.organization != org {
                return Err("Forbidden: organization mismatch".into());
            }
            return Ok(value);
        }
    }
    if crate::auth::configured() {
        let (subject, organization, role) = crate::auth::verify(token.ok_or("Unauthorized")?)?;
        if organization != org {
            return Err("Forbidden: organization mismatch".into());
        }
        let role = match role.as_str() {
            "admin" => Role::Admin,
            "manager" => Role::Manager,
            "editor" => Role::Editor,
            _ => Role::Viewer,
        };
        return Ok(Identity {
            subject,
            organization,
            role,
        });
    }
    if config.is_some() {
        return Err("Unauthorized".into());
    }
    Ok(Identity {
        subject: "local".into(),
        organization: org.into(),
        role: Role::Admin,
    })
}

fn header<'a>(req: &'a Request, key: &str) -> Option<&'a str> {
    req.headers
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
}
fn permit(id: &Identity, role: Role) -> Result<(), String> {
    if id.role < role {
        Err("Forbidden: insufficient role".into())
    } else {
        Ok(())
    }
}
fn value(body: &str) -> Result<Value, String> {
    serde_json::from_str(body).map_err(|e| format!("Invalid JSON: {e}"))
}
fn body_revision(v: &Value) -> Result<u64, String> {
    v["base_revision"]
        .as_u64()
        .ok_or("Missing base_revision".into())
}
fn route(store: &mut Store, req: &Request) -> Result<(String, String), String> {
    if req.method == "GET" && req.path == "/api/auth/config" {
        return Ok((
            "application/json; charset=utf-8".into(),
            crate::auth::config()?.to_string(),
        ));
    }
    let identity = identity(req)?;
    let org = &identity.organization;
    let project = header(req, "x-project-id").unwrap_or("demo");
    let method = req.method.as_str();
    let path = req.path.as_str();
    let mut result = None;
    match (method, path) {
        ("GET", "/api/capabilities") => {
            result = Some(
                json!({"persistent":true,"organization":org,"role":identity.role,"subject":identity.subject,"authentication":if crate::auth::configured(){"oidc"}else if env::var("FORGEPLAN_IDENTITIES").is_ok(){"static_api_tokens"}else{"loopback_development"}}),
            )
        }
        ("GET", "/api/projects") | ("GET", "/api/portfolio") => result = Some(store.list(org)?),
        ("POST", "/api/projects") => {
            permit(&identity, Role::Admin)?;
            let v = value(&req.body)?;
            let id = v["id"].as_str().ok_or("Missing project id")?;
            let mut p = if let Some(text) = v["document"].as_str() {
                parse(text)?
            } else {
                parse(DEMO)?
            };
            if let Some(title) = v["title"].as_str() {
                p.title = title.into();
            }
            store.create(org, id, p, &identity.subject)?;
            result = Some(json!({"id":id}));
        }
        ("GET", "/api/state") => {
            return Ok((
                "application/json; charset=utf-8".into(),
                store.editor(org, project)?.json(),
            ))
        }
        ("GET", "/api/project") => {
            return Ok((
                "application/json; charset=utf-8".into(),
                serialize(store.editor(org, project)?.project())?,
            ))
        }
        ("POST", "/api/commands") => {
            permit(&identity, Role::Editor)?;
            let key = header(req, "x-command-id").ok_or("Missing X-Command-ID")?;
            return Ok((
                "application/json; charset=utf-8".into(),
                store.mutate(org, project, &identity.subject, key, &req.body)?,
            ));
        }
        ("POST", "/api/schedule") => {
            let p = parse(&req.body)?;
            return Ok((
                "application/json; charset=utf-8".into(),
                schedule_json(&p, &schedule(&p)?),
            ));
        }
        ("GET", "/api/audit") => result = Some(store.audit(org, project)?),
        ("GET", "/api/published") => result = Some(store.published(org, project)?),
        ("POST", "/api/publish") => {
            permit(&identity, Role::Manager)?;
            result = Some(store.publish(
                org,
                project,
                &identity.subject,
                body_revision(&value(&req.body)?)?,
            )?);
        }
        ("POST", "/api/leveling/preview") => {
            permit(&identity, Role::Editor)?;
            let v = value(&req.body)?;
            let e = store.editor(org, project)?;
            let preview = plan_core::leveling::preview(
                e.project(),
                v["within_float"].as_bool().unwrap_or(false),
            )?;
            result = Some(json!({"base_revision":e.revision().get(),"preview":preview}));
        }
        ("POST", "/api/leveling/apply") => {
            permit(&identity, Role::Editor)?;
            let v = value(&req.body)?;
            let e = store.editor(org, project)?;
            let expected = body_revision(&v)?;
            if expected != e.revision().get() {
                return Err("Revision conflict: leveling preview is stale".into());
            }
            let preview = plan_core::leveling::preview(
                e.project(),
                v["within_float"].as_bool().unwrap_or(false),
            )?;
            if preview.unresolved > 0 {
                return Err("Leveling has unresolved conflicts".into());
            }
            let body=json!({"base_revision":expected,"commands":[plan_core::Command::Replace(Box::new(preview.project))]}).to_string();
            let key = header(req, "x-command-id").ok_or("Missing X-Command-ID")?;
            return Ok((
                "application/json; charset=utf-8".into(),
                store.mutate(org, project, &identity.subject, key, &body)?,
            ));
        }
        ("GET", "/api/export.csv") => {
            return Ok((
                "text/csv; charset=utf-8".into(),
                plan_core::exchange::export_csv(store.editor(org, project)?.project())?,
            ))
        }
        ("GET", "/api/export.xml") => {
            result = Some(
                serde_json::to_value(plan_core::exchange::export_xml(
                    store.editor(org, project)?.project(),
                )?)
                .map_err(|e| e.to_string())?,
            )
        }
        ("POST", "/api/import.xml") => {
            permit(&identity, Role::Editor)?;
            let imported = plan_core::exchange::import_xml(&req.body)?;
            result = Some(
                json!({"document":serialize(&imported.project)?,"warnings":imported.warnings}),
            );
        }
        ("POST", "/api/import.csv") => {
            permit(&identity, Role::Editor)?;
            let imported = plan_core::exchange::import_csv(&req.body)?;
            result = Some(
                json!({"document":serialize(&imported.project)?,"warnings":imported.warnings}),
            );
        }
        ("GET", "/api/timesheets") => result = Some(store.timesheets(org, project)?),
        ("POST", "/api/timesheets") => {
            permit(&identity, Role::Editor)?;
            result =
                Some(store.submit_time(org, project, &identity.subject, &value(&req.body)?)?);
        }
        ("POST", "/api/timesheets/approve") => {
            permit(&identity, Role::Manager)?;
            let v = value(&req.body)?;
            result = Some(store.approve_time(
                org,
                project,
                &identity.subject,
                v["id"].as_u64().ok_or("Missing timesheet id")?,
            )?);
        }
        _ => {}
    }
    result
        .map(|v| ("application/json; charset=utf-8".into(), v.to_string()))
        .ok_or("Not found".into())
}
pub fn run(port: u16, path: &str) -> Result<(), String> {
    run_inner(port, path, false, false)
}
pub fn run_desktop(port: u16, path: &str, open_browser: bool) -> Result<(), String> {
    run_inner(port, path, true, open_browser)
}
fn run_inner(port: u16, path: &str, desktop: bool, open_browser: bool) -> Result<(), String> {
    if desktop
        && (env::var("FORGEPLAN_BIND").is_ok_and(|v| v != "127.0.0.1")
            || env::var_os("FORGEPLAN_PUBLIC_ORIGIN").is_some())
    {
        return Err("Desktop requires loopback binding without FORGEPLAN_PUBLIC_ORIGIN; use workspace for server deployment".into());
    }
    let mut store = Store::open(path)?;
    if store.editor("local", "demo").is_err() {
        store.create("local", "demo", parse(DEMO)?, "bootstrap")?;
    }
    if let Ok(config) = env::var("FORGEPLAN_IDENTITIES") {
        let identities: BTreeMap<String, Identity> =
            serde_json::from_str(&config).map_err(|_| "Invalid FORGEPLAN_IDENTITIES JSON")?;
        if identities.is_empty() || identities.keys().any(|key| key.len() < 32) {
            return Err("Configured API tokens must contain at least 32 characters".into());
        }
    }
    let bind = env::var("FORGEPLAN_BIND").unwrap_or("127.0.0.1".into());
    if bind != "127.0.0.1"
        && env::var("FORGEPLAN_IDENTITIES").is_err()
        && !crate::auth::configured()
    {
        return Err("Non-loopback binding requires FORGEPLAN_IDENTITIES".into());
    }
    if crate::auth::configured() {
        crate::auth::config()?;
    }
    let listener = TcpListener::bind((bind.as_str(), port)).map_err(|e| e.to_string())?;
    let host = format!(
        "127.0.0.1:{}",
        listener.local_addr().map_err(|e| e.to_string())?.port()
    );
    let origin = env::var("FORGEPLAN_PUBLIC_ORIGIN").unwrap_or(format!("http://{host}"));
    let origin = origin.trim_end_matches('/').to_string();
    let host = origin
        .strip_prefix("https://")
        .or_else(|| origin.strip_prefix("http://"))
        .filter(|s| !s.is_empty() && !s.contains(['/', '\r', '\n']))
        .ok_or("Invalid FORGEPLAN_PUBLIC_ORIGIN")?
        .to_string();
    if bind != "127.0.0.1"
        && !origin.starts_with("https://")
        && !origin.starts_with("http://127.0.0.1:")
        && !origin.starts_with("http://localhost:")
    {
        return Err("Remote deployments require HTTPS public origin".into());
    }
    println!("Open {origin}\nPersistent SQLite workspace. Database: {path}\nCtrl+C to stop.");
    if open_browser {
        let url = origin.clone();
        std::thread::spawn(move || {
            if let Err(error) = crate::platform::open_browser(&url) {
                eprintln!("Browser launch failed: {error}. Open {url} manually.");
            }
        });
    }
    for stream in listener.incoming() {
        let mut stream = stream.map_err(|e| e.to_string())?;
        let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
        let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
        let req = match read_request(&mut stream) {
            Ok(req) => req,
            Err(e) => {
                let _ = respond(
                    &mut stream,
                    "400 Bad Request",
                    "application/json",
                    &json!({"error":e}).to_string(),
                );
                continue;
            }
        };
        let output = if header(&req, "host") != Some(host.as_str()) {
            Err("Forbidden: use exact loopback URL".into())
        } else if req.method == "POST" && header(&req, "origin") != Some(origin.as_str()) {
            Err("Forbidden: same origin required".into())
        } else if req.method == "GET" && req.path.split('?').next() == Some("/") {
            Ok(("text/html; charset=utf-8".into(), UI.into()))
        } else if req.method == "GET" && req.path == "/app.js" {
            Ok(("text/javascript; charset=utf-8".into(), JS.into()))
        } else {
            route(&mut store, &req)
        };
        match output {
            Ok((kind, body)) => {
                let _ = respond(&mut stream, "200 OK", &kind, &body);
            }
            Err(error) => {
                let code = if error == "Unauthorized" {
                    "401 Unauthorized"
                } else if error.starts_with("Forbidden:") {
                    "403 Forbidden"
                } else if error == "Not found" {
                    "404 Not Found"
                } else if error.starts_with("Revision conflict:") {
                    "409 Conflict"
                } else {
                    "422 Unprocessable Entity"
                };
                let _ = respond(
                    &mut stream,
                    code,
                    "application/json; charset=utf-8",
                    &json!({"error":error}).to_string(),
                );
            }
        }
    }
    Ok(())
}
