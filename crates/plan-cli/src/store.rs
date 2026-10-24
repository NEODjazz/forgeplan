use plan_core::*;
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
pub struct Store {
    db: Connection,
}
impl Store {
    pub fn open(path: &str) -> Result<Self> {
        let db = Connection::open(path).map_err(|e| e.to_string())?;
        db.busy_timeout(std::time::Duration::from_secs(5))
            .map_err(|e| e.to_string())?;
        db.execute_batch("PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL;")
            .map_err(|e| e.to_string())?;
        let version: i64 = db
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .map_err(|e| e.to_string())?;
        if version == 0 {
            db.execute_batch(include_str!("../../../migrations/001_sqlite.sql"))
                .map_err(|e| e.to_string())?;
        } else if version != 1 {
            return Err("Unsupported workspace schema version".into());
        }
        Ok(Self { db })
    }
    pub fn create(&mut self, org: &str, id: &str, project: Project, actor: &str) -> Result<()> {
        valid_id(org)?;
        valid_id(id)?;
        let e = Editor::new(project)?;
        let snapshot = serde_json::to_string(&e).map_err(|e| e.to_string())?;
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| e.to_string())?;
        tx.execute(
            "INSERT INTO projects(org,id,revision,snapshot) VALUES(?1,?2,0,?3)",
            params![org, id, snapshot],
        )
        .map_err(|e| format!("Cannot create project: {e}"))?;
        tx.execute(
            "INSERT INTO audit(org,project,revision,actor,action) VALUES(?1,?2,0,?3,'create')",
            params![org, id, actor],
        )
        .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())
    }
    pub fn editor(&self, org: &str, id: &str) -> Result<Editor> {
        let snapshot: String = self
            .db
            .query_row(
                "SELECT snapshot FROM projects WHERE org=?1 AND id=?2",
                params![org, id],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?
            .ok_or("Not found")?;
        decode(&snapshot)
    }
    pub fn list(&self, org: &str) -> Result<Value> {
        let mut q=self.db.prepare("SELECT id,snapshot,revision,published IS NOT NULL FROM projects WHERE org=?1 ORDER BY id").map_err(|e|e.to_string())?;
        let rows = q
            .query_map([org], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, u64>(2)?,
                    r.get::<_, bool>(3)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        let mut projects = Vec::new();
        for row in rows {
            let (id, snapshot, revision, published) = row.map_err(|e| e.to_string())?;
            let e = decode(&snapshot)?;
            let s = schedule(e.project())?;
            let costs = management::planned_costs(e.project())?;
            projects.push(json!({"id":id,"title":e.project().title,"revision":revision,"published":published,"working_minutes":s.finish,"planned_cost_cents":costs.values().sum::<i64>(),"resources":e.project().resources.len(),"tasks":e.project().tasks.len()}));
        }
        Ok(json!({"projects":projects}))
    }
    pub fn mutate(
        &mut self,
        org: &str,
        id: &str,
        actor: &str,
        command_id: &str,
        body: &str,
    ) -> Result<String> {
        if command_id.is_empty() || command_id.len() > 100 {
            return Err("Command ID must contain 1..100 characters".into());
        }
        let digest = format!("{:x}", Sha256::digest(body.as_bytes()));
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| e.to_string())?;
        let prior:Option<(String,String)>=tx.query_row("SELECT digest,response FROM receipts WHERE org=?1 AND project=?2 AND actor=?3 AND command_id=?4",params![org,id,actor,command_id],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(|e|e.to_string())?;
        if let Some((old, response)) = prior {
            return if old == digest {
                Ok(response)
            } else {
                Err("Revision conflict: command ID reused with another payload".into())
            };
        }
        let snapshot: String = tx
            .query_row(
                "SELECT snapshot FROM projects WHERE org=?1 AND id=?2",
                params![org, id],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?
            .ok_or("Not found")?;
        let mut editor = decode(&snapshot)?;
        let (rev, action) = crate::protocol::parse_edit(body)?;
        let old = editor.revision().get();
        match action {
            crate::protocol::Action::Batch(commands) => editor.apply(rev, commands)?,
            crate::protocol::Action::Undo => editor.undo(rev)?,
            crate::protocol::Action::Redo => editor.redo(rev)?,
        };
        editor.compact_history(60_000_000)?;
        let response = editor.json();
        let snapshot = serde_json::to_string(&editor).map_err(|e| e.to_string())?;
        if snapshot.len() > 64_000_000 {
            return Err(
                "Durable editor history exceeds 64 MB; export and reopen a clean project".into(),
            );
        }
        let changed=tx.execute("UPDATE projects SET revision=?1,snapshot=?2 WHERE org=?3 AND id=?4 AND revision=?5",params![editor.revision().get(),snapshot,org,id,old]).map_err(|e|e.to_string())?;
        if changed != 1 {
            return Err("Revision conflict: concurrent database update".into());
        }
        tx.execute(
            "INSERT INTO audit(org,project,revision,actor,action) VALUES(?1,?2,?3,?4,'command')",
            params![org, id, editor.revision().get(), actor],
        )
        .map_err(|e| e.to_string())?;
        tx.execute(
            "INSERT INTO receipts VALUES(?1,?2,?3,?4,?5,?6)",
            params![org, id, actor, command_id, digest, response],
        )
        .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
        Ok(response)
    }
    pub fn publish(&mut self, org: &str, id: &str, actor: &str, revision: u64) -> Result<Value> {
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| e.to_string())?;
        let (snapshot, current): (String, u64) = tx
            .query_row(
                "SELECT snapshot,revision FROM projects WHERE org=?1 AND id=?2",
                params![org, id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(|e| e.to_string())?
            .ok_or("Not found")?;
        if current != revision {
            return Err("Revision conflict: publish requires the current revision".into());
        }
        let e = decode(&snapshot)?;
        let published = json!({"revision":revision,"document":serialize(e.project())?});
        tx.execute("INSERT OR IGNORE INTO publications(org,project,revision,document,actor) VALUES(?1,?2,?3,?4,?5)",params![org,id,revision,serialize(e.project())?,actor]).map_err(|e|e.to_string())?;
        tx.execute(
            "UPDATE projects SET published=?1 WHERE org=?2 AND id=?3",
            params![published.to_string(), org, id],
        )
        .map_err(|e| e.to_string())?;
        tx.execute(
            "INSERT INTO audit(org,project,revision,actor,action) VALUES(?1,?2,?3,?4,'publish')",
            params![org, id, revision, actor],
        )
        .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
        Ok(published)
    }
    pub fn published(&self, org: &str, id: &str) -> Result<Value> {
        let text: Option<String> = self
            .db
            .query_row(
                "SELECT published FROM projects WHERE org=?1 AND id=?2",
                params![org, id],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?
            .flatten();
        text.map(|s| serde_json::from_str(&s).map_err(|e| e.to_string()))
            .unwrap_or(Err("Not found".into()))
    }
    pub fn audit(&self, org: &str, id: &str) -> Result<Value> {
        let mut q=self.db.prepare("SELECT seq,revision,actor,action,at FROM audit WHERE org=?1 AND project=?2 ORDER BY seq DESC LIMIT 100").map_err(|e|e.to_string())?;
        let rows=q.query_map(params![org,id],|r|Ok(json!({"seq":r.get::<_,u64>(0)?,"revision":r.get::<_,u64>(1)?,"actor":r.get::<_,String>(2)?,"action":r.get::<_,String>(3)?,"at":r.get::<_,String>(4)?}))).map_err(|e|e.to_string())?;
        Ok(
            json!({"events":rows.collect::<std::result::Result<Vec<_>,_>>().map_err(|e|e.to_string())?}),
        )
    }
    pub fn submit_time(
        &mut self,
        org: &str,
        id: &str,
        actor: &str,
        value: &Value,
    ) -> Result<Value> {
        let task: TaskId =
            serde_json::from_value(value["task"].clone()).map_err(|e| e.to_string())?;
        let resource: ResourceId =
            serde_json::from_value(value["resource"].clone()).map_err(|e| e.to_string())?;
        let date = Date::parse(value["date"].as_str().ok_or("Missing date")?)?;
        let minutes = value["minutes"]
            .as_i64()
            .filter(|m| (1..=1440).contains(m))
            .ok_or("Minutes must be 1..1440")?;
        let e = self.editor(org, id)?;
        if !e
            .project()
            .assignments
            .iter()
            .any(|a| a.task == task && a.resource == resource)
        {
            return Err("Timesheet requires a current assignment".into());
        }
        self.db.execute("INSERT INTO timesheets(org,project,task,resource,day,minutes,actor) VALUES(?1,?2,?3,?4,?5,?6,?7)",params![org,id,task.get(),resource.get(),date.iso(),minutes,actor]).map_err(|e|e.to_string())?;
        Ok(json!({"id":self.db.last_insert_rowid(),"status":"submitted"}))
    }
    pub fn timesheets(&self, org: &str, id: &str) -> Result<Value> {
        let mut q=self.db.prepare("SELECT id,task,resource,day,minutes,actor,status FROM timesheets WHERE org=?1 AND project=?2 ORDER BY id DESC LIMIT 200").map_err(|e|e.to_string())?;
        let rows=q.query_map(params![org,id],|r|Ok(json!({"id":r.get::<_,u64>(0)?,"task":r.get::<_,u32>(1)?,"resource":r.get::<_,u32>(2)?,"date":r.get::<_,String>(3)?,"minutes":r.get::<_,u32>(4)?,"actor":r.get::<_,String>(5)?,"status":r.get::<_,String>(6)?}))).map_err(|e|e.to_string())?;
        Ok(
            json!({"timesheets":rows.collect::<std::result::Result<Vec<_>,_>>().map_err(|e|e.to_string())?}),
        )
    }
    pub fn approve_time(&mut self, org: &str, id: &str, actor: &str, entry: u64) -> Result<Value> {
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| e.to_string())?;
        let(task,resource,minutes,owner,status):(u32,u32,i64,String,String)=tx.query_row("SELECT task,resource,minutes,actor,status FROM timesheets WHERE org=?1 AND project=?2 AND id=?3",params![org,id,entry],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).optional().map_err(|e|e.to_string())?.ok_or("Not found")?;
        if owner == actor {
            return Err("Forbidden: self approval is not allowed".into());
        }
        if status != "submitted" {
            return Err("Revision conflict: timesheet already processed".into());
        }
        let snapshot: String = tx
            .query_row(
                "SELECT snapshot FROM projects WHERE org=?1 AND id=?2",
                params![org, id],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        let mut e = decode(&snapshot)?;
        let task = TaskId::new(task)?;
        let resource = ResourceId::new(resource)?;
        if !e
            .project()
            .assignments
            .iter()
            .any(|a| a.task == task && a.resource == resource)
        {
            return Err("Timesheet assignment no longer exists".into());
        }
        let mut actual = e
            .project()
            .management
            .actuals
            .get(&task)
            .cloned()
            .unwrap_or_default();
        actual.work_basis = actual
            .work_basis
            .checked_add(minutes * 10000)
            .ok_or("Work overflow")?;
        let rate = *e
            .project()
            .management
            .hourly_rates_cents
            .get(&resource)
            .unwrap_or(&0);
        actual.cost_cents = actual
            .cost_cents
            .checked_add(minutes * rate / 60)
            .ok_or("Cost overflow")?;
        e.apply(e.revision(), vec![Command::Actual { id: task, actual }])?;
        e.compact_history(60_000_000)?;
        tx.execute(
            "UPDATE projects SET revision=?1,snapshot=?2 WHERE org=?3 AND id=?4",
            params![
                e.revision().get(),
                serde_json::to_string(&e).map_err(|e| e.to_string())?,
                org,
                id
            ],
        )
        .map_err(|e| e.to_string())?;
        tx.execute(
            "UPDATE timesheets SET status='approved' WHERE id=?1 AND org=?2 AND project=?3",
            params![entry, org, id],
        )
        .map_err(|e| e.to_string())?;
        tx.execute("INSERT INTO audit(org,project,revision,actor,action) VALUES(?1,?2,?3,?4,'approve_timesheet')",params![org,id,e.revision().get(),actor]).map_err(|e|e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
        Ok(json!({"status":"approved","revision":e.revision().get()}))
    }
    pub fn backup(&self, path: &str) -> Result<()> {
        if std::path::Path::new(path).exists() {
            return Err("Backup destination already exists".into());
        }
        self.db
            .backup("main", path, None)
            .map_err(|e| e.to_string())
    }
}
fn decode(snapshot: &str) -> Result<Editor> {
    if snapshot.len() > 64_000_000 {
        return Err("Snapshot exceeds 64 MB".into());
    }
    let e: Editor = serde_json::from_str(snapshot).map_err(|e| e.to_string())?;
    e.validate_snapshot()?;
    Ok(e)
}
fn valid_id(id: &str) -> Result<()> {
    if id.is_empty()
        || id.len() > 80
        || !id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
    {
        Err("ID must contain 1..80 letters/digits/dashes/underscores".into())
    } else {
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn project() -> Project {
        parse(crate::DEMO).unwrap()
    }
    fn command(revision: u64, commands: Vec<Command>) -> String {
        json!({"base_revision":revision,"commands":commands}).to_string()
    }
    #[test]
    fn durable_history_retry_revision_and_organization_isolation() {
        let path = std::env::temp_dir().join(format!(
            "forgeplan-store-{}-{}.sqlite",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let path = path.to_str().unwrap();
        {
            let mut store = Store::open(path).unwrap();
            store.create("a", "demo", project(), "owner").unwrap();
            store.create("b", "demo", project(), "owner").unwrap();
            let body = command(
                0,
                vec![Command::Rate {
                    id: ResourceId::new(1).unwrap(),
                    cents_per_hour: 6000,
                }],
            );
            let first = store
                .mutate("a", "demo", "user", "command-1", &body)
                .unwrap();
            assert_eq!(
                store
                    .mutate("a", "demo", "user", "command-1", &body)
                    .unwrap(),
                first
            );
            assert!(store
                .mutate("a", "demo", "user", "command-1", &command(0, vec![]))
                .is_err());
            assert!(store
                .mutate("a", "demo", "user", "command-2", &body)
                .is_err());
            assert_eq!(store.editor("b", "demo").unwrap().revision().get(), 0);
            store.publish("a", "demo", "manager", 1).unwrap();
        }
        let mut store = Store::open(path).unwrap();
        assert_eq!(store.editor("a", "demo").unwrap().revision().get(), 1);
        store
            .mutate("a", "demo", "user", "undo", "revision\t1\nundo\n")
            .unwrap();
        assert_eq!(store.editor("a", "demo").unwrap().revision().get(), 2);
        assert_eq!(store.published("a", "demo").unwrap()["revision"], 1);
        let _ = std::fs::remove_file(path);
    }
    #[test]
    fn timesheet_approval_is_separate_and_applies_actual_once() {
        let mut s = Store::open(":memory:").unwrap();
        s.create("org", "p", project(), "admin").unwrap();
        let entry = s
            .submit_time(
                "org",
                "p",
                "worker",
                &json!({"task":1,"resource":1,"date":"2026-10-26","minutes":60}),
            )
            .unwrap()["id"]
            .as_u64()
            .unwrap();
        assert!(s.approve_time("org", "p", "worker", entry).is_err());
        s.approve_time("org", "p", "manager", entry).unwrap();
        assert_eq!(
            s.editor("org", "p").unwrap().project().management.actuals[&TaskId::new(1).unwrap()]
                .work_basis,
            600000
        );
        assert!(s.approve_time("org", "p", "manager", entry).is_err());
    }
    #[test]
    fn backup_can_be_reopened_and_preserves_project() {
        let mut s = Store::open(":memory:").unwrap();
        s.create("org", "p", project(), "owner").unwrap();
        let path =
            std::env::temp_dir().join(format!("forgeplan-backup-{}.sqlite", std::process::id()));
        let _ = std::fs::remove_file(&path);
        s.backup(path.to_str().unwrap()).unwrap();
        let restored = Store::open(path.to_str().unwrap()).unwrap();
        assert_eq!(
            restored.list("org").unwrap()["projects"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        let _ = std::fs::remove_file(path);
    }
}
