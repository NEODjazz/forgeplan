use crate::*;
use serde::{Deserialize, Serialize};
pub const SCHEMA_VERSION: u32 = 1;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    format: String,
    schema_version: u32,
    project: Project,
}
pub fn encode(p: &Project) -> Result<String> {
    schedule(p)?;
    let mut project = p.clone();
    project.tasks = outline(p)?
        .iter()
        .map(|row| p.tasks[row.index].clone())
        .collect();
    project
        .links
        .sort_by_key(|l| (l.to, l.from, l.relation.label(), l.lag.minutes()));
    project.resources.sort_by_key(|r| r.id);
    for r in &mut project.resources {
        r.availability.sort_by_key(|a| a.from);
    }
    project.assignments.sort_by_key(|a| (a.task, a.resource));
    let text = serde_json::to_string_pretty(&Document {
        format: "forgeplan".into(),
        schema_version: SCHEMA_VERSION,
        project,
    })
    .map_err(|e| e.to_string())?;
    if text.len() > 8_000_000 {
        return Err("Project exceeds 8 MB JSON limit".into());
    }
    Ok(text + "\n")
}
pub fn decode(text: &str) -> Result<Project> {
    if text.len() > 8_000_000 {
        return Err("Project exceeds 8 MB JSON limit".into());
    }
    let doc: Document =
        serde_json::from_str(text).map_err(|e| format!("Invalid ForgePlan JSON: {e}"))?;
    if doc.format != "forgeplan" || doc.schema_version != SCHEMA_VERSION {
        return Err("Unsupported ForgePlan document schema".into());
    }
    schedule(&doc.project)?;
    Ok(doc.project)
}

#[derive(Deserialize)]
#[serde(untagged)]
pub enum IdWire {
    Number(u32),
    Text(String),
}
impl IdWire {
    pub fn value(self) -> Result<u32> {
        match self {
            Self::Number(n) => Ok(n),
            Self::Text(s) => {
                if s.is_empty() || !s.bytes().all(|c| c.is_ascii_digit()) {
                    return Err("Invalid numeric ID".into());
                }
                s.parse().map_err(|_| "Invalid numeric ID".into())
            }
        }
    }
}
