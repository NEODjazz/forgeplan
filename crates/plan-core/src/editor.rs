//! Atomic domain commands and bounded in-memory undo/redo.
use crate::*;
pub const HISTORY_LIMIT: usize = 50;
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct Revision(u64);
impl Revision {
    pub fn new(value: u64) -> Self {
        Self(value)
    }
    pub fn get(self) -> u64 {
        self.0
    }
    fn next(self) -> Result<Self> {
        self.0
            .checked_add(1)
            .map(Self)
            .ok_or("Revision exhausted".into())
    }
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(
    tag = "type",
    content = "data",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Command {
    Add {
        task: Task,
        before: Option<TaskId>,
    },
    Update {
        id: TaskId,
        title: String,
        duration: WorkDuration,
    },
    Move {
        id: TaskId,
        parent: Option<TaskId>,
        before: Option<TaskId>,
    },
    DeleteSubtree {
        id: TaskId,
    },
    SetPredecessors {
        id: TaskId,
        links: Vec<Dependency>,
    },
    ProjectSettings {
        title: String,
        start: Date,
        holidays: BTreeSet<Date>,
    },
    CalendarSettings {
        start_minute: u16,
        utc_offset: i16,
        weekly: [Vec<WorkInterval>; 7],
        exceptions: BTreeMap<Date, Vec<WorkInterval>>,
    },
    Dates {
        id: TaskId,
        constraint: Option<Constraint>,
        deadline: Option<Moment>,
    },
    AddResource(Resource),
    UpdateResource(Resource),
    DeleteResource(ResourceId),
    SetAssignments {
        task: TaskId,
        assignments: Vec<Assignment>,
    },
    Policy {
        id: TaskId,
        policy: management::TaskPolicy,
    },
    Work {
        id: TaskId,
        target_basis: i64,
    },
    PersonalCalendar {
        id: ResourceId,
        calendar: Option<management::PersonalCalendar>,
    },
    Rate {
        id: ResourceId,
        cents_per_hour: i64,
    },
    FixedCost {
        id: TaskId,
        cents: i64,
    },
    Actual {
        id: TaskId,
        actual: management::Actual,
    },
    Baseline {
        name: String,
    },
    StatusDate(Option<Moment>),
    Replace(Box<Project>),
}
#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Editor {
    project: Project,
    revision: Revision,
    undo: Vec<Project>,
    redo: Vec<Project>,
    used: BTreeSet<TaskId>,
    used_resources: BTreeSet<ResourceId>,
}
impl Editor {
    pub fn new(mut project: Project) -> Result<Self> {
        project
            .links
            .sort_by_key(|l| (l.to, l.from, l.relation.label(), l.lag.minutes()));
        serialize(&project)?;
        let used = project.tasks.iter().map(|t| t.id).collect();
        let used_resources = project.resources.iter().map(|r| r.id).collect();
        Ok(Self {
            project,
            revision: Revision(0),
            undo: vec![],
            redo: vec![],
            used,
            used_resources,
        })
    }
    pub fn validate_snapshot(&self) -> Result<()> {
        serialize(&self.project)?;
        for p in self.undo.iter().chain(&self.redo) {
            serialize(p)?;
        }
        if self.undo.len() > HISTORY_LIMIT || self.redo.len() > HISTORY_LIMIT {
            return Err("Invalid editor history".into());
        }
        Ok(())
    }
    pub fn compact_history(&mut self, budget: usize) -> Result<()> {
        loop {
            if serde_json::to_vec(self).map_err(|e| e.to_string())?.len() <= budget {
                return Ok(());
            }
            if !self.undo.is_empty() {
                self.undo.remove(0);
            } else if !self.redo.is_empty() {
                self.redo.remove(0);
            } else {
                return Err("Current editor state exceeds persistence budget".into());
            }
        }
    }
    pub fn project(&self) -> &Project {
        &self.project
    }
    pub fn revision(&self) -> Revision {
        self.revision
    }
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    pub fn next_id(&self) -> Option<TaskId> {
        self.used
            .last()
            .map_or(Some(1), |id| id.get().checked_add(1))
            .and_then(|v| TaskId::new(v).ok())
    }
    pub fn next_resource_id(&self) -> Option<ResourceId> {
        self.used_resources
            .last()
            .map_or(Some(1), |id| id.get().checked_add(1))
            .and_then(|id| ResourceId::new(id).ok())
    }
    fn check_revision(&self, expected: Revision) -> Result<()> {
        if expected != self.revision {
            Err(format!(
                "Revision conflict: expected {}, current {}",
                expected.get(),
                self.revision.get()
            ))
        } else {
            Ok(())
        }
    }
    pub fn apply(&mut self, expected: Revision, commands: Vec<Command>) -> Result<()> {
        self.check_revision(expected)?;
        if commands.is_empty() || commands.len() > 10000 {
            return Err("Expected 1..10000 commands".into());
        }
        let mut draft = self.project.clone();
        let mut used = self.used.clone();
        let mut used_resources = self.used_resources.clone();
        for command in commands {
            apply_command(&mut draft, &mut used, &mut used_resources, command)?;
        }
        draft
            .links
            .sort_by_key(|l| (l.to, l.from, l.relation.label(), l.lag.minutes()));
        for old in &self.project.management.baselines {
            if draft
                .management
                .baselines
                .iter()
                .any(|new| new.name == old.name && new != old)
            {
                return Err("Existing baseline snapshots cannot be modified in place".into());
            }
        }
        if serialize(&draft)? == serialize(&self.project)? {
            return Ok(());
        }
        let next = self.revision.next()?;
        push_history(&mut self.undo, self.project.clone());
        self.redo.clear();
        self.project = draft;
        self.used = used;
        self.used_resources = used_resources;
        self.revision = next;
        Ok(())
    }
    pub fn undo(&mut self, expected: Revision) -> Result<()> {
        self.check_revision(expected)?;
        let next = self.revision.next()?;
        let previous = self.undo.pop().ok_or("Nothing to undo")?;
        push_history(
            &mut self.redo,
            std::mem::replace(&mut self.project, previous),
        );
        self.revision = next;
        Ok(())
    }
    pub fn redo(&mut self, expected: Revision) -> Result<()> {
        self.check_revision(expected)?;
        let next = self.revision.next()?;
        let following = self.redo.pop().ok_or("Nothing to redo")?;
        push_history(
            &mut self.undo,
            std::mem::replace(&mut self.project, following),
        );
        self.revision = next;
        Ok(())
    }
    pub fn json(&self) -> String {
        let s = schedule(&self.project).expect("validated editor");
        format!("{{\"revision\":{},\"can_undo\":{},\"can_redo\":{},\"next_id\":{},\"next_resource_id\":{},\"document\":{},\"schedule\":{}}}",self.revision.get(),self.can_undo(),self.can_redo(),self.next_id().map(|id|id.to_string()).unwrap_or("null".into()),self.next_resource_id().map(|id|id.to_string()).unwrap_or("null".into()),json_string(&serialize(&self.project).expect("validated document")),schedule_json(&self.project,&s))
    }
}
fn push_history(history: &mut Vec<Project>, p: Project) {
    if history.len() == HISTORY_LIMIT {
        history.remove(0);
    }
    history.push(p);
}
fn task_mut(p: &mut Project, id: TaskId) -> Result<&mut Task> {
    p.tasks
        .iter_mut()
        .find(|t| t.id == id)
        .ok_or_else(|| format!("Unknown task {id}"))
}
fn siblings(p: &Project, parent: Option<TaskId>, exclude: Option<TaskId>) -> Vec<TaskId> {
    let mut tasks: Vec<_> = p
        .tasks
        .iter()
        .filter(|t| t.parent == parent && Some(t.id) != exclude)
        .collect();
    tasks.sort_by_key(|t| (t.order, t.id));
    tasks.iter().map(|t| t.id).collect()
}
fn set_order(p: &mut Project, ids: &[TaskId]) {
    for (order, &id) in ids.iter().enumerate() {
        p.tasks
            .iter_mut()
            .find(|t| t.id == id)
            .expect("known sibling")
            .order = order as u32;
    }
}
fn subtree(p: &Project, id: TaskId) -> Result<BTreeSet<TaskId>> {
    if !p.tasks.iter().any(|t| t.id == id) {
        return Err(format!("Unknown task {id}"));
    }
    let mut ids = BTreeSet::from([id]);
    loop {
        let len = ids.len();
        for t in &p.tasks {
            if t.parent.is_some_and(|parent| ids.contains(&parent)) {
                ids.insert(t.id);
            }
        }
        if len == ids.len() {
            return Ok(ids);
        }
    }
}
fn place(
    p: &mut Project,
    id: TaskId,
    parent: Option<TaskId>,
    before: Option<TaskId>,
) -> Result<()> {
    let descendants = subtree(p, id)?;
    if let Some(parent) = parent {
        if descendants.contains(&parent) {
            return Err("Hierarchy cycle: cannot move a task into its subtree".into());
        }
        if !p
            .tasks
            .iter()
            .any(|t| t.id == parent && t.kind == TaskKind::Summary)
        {
            return Err("Target parent must be a summary task".into());
        }
    }
    let old_parent = p
        .tasks
        .iter()
        .find(|t| t.id == id)
        .ok_or("Unknown task")?
        .parent;
    let mut destination = siblings(p, parent, Some(id));
    let position = if let Some(before) = before {
        destination
            .iter()
            .position(|&x| x == before)
            .ok_or("Before task must be a different sibling in the target parent")?
    } else {
        destination.len()
    };
    destination.insert(position, id);
    task_mut(p, id)?.parent = parent;
    if old_parent != parent {
        let old = siblings(p, old_parent, Some(id));
        set_order(p, &old);
    }
    set_order(p, &destination);
    Ok(())
}
fn apply_command(
    p: &mut Project,
    used: &mut BTreeSet<TaskId>,
    used_resources: &mut BTreeSet<ResourceId>,
    command: Command,
) -> Result<()> {
    match command {
        Command::Add { mut task, before } => {
            if p.tasks.len() >= MAX_TASKS {
                return Err("Task limit reached".into());
            }
            if !used.insert(task.id) {
                return Err("Task ID was already used; allocate next_id".into());
            }
            let id = task.id;
            let parent = task.parent;
            task.parent = None;
            task.order = u32::MAX;
            p.tasks.push(task);
            place(p, id, parent, before)?;
        }
        Command::Update {
            id,
            title,
            duration,
        } => {
            let t = task_mut(p, id)?;
            t.title = title;
            t.duration = duration;
        }
        Command::Move { id, parent, before } => {
            outline(p)?;
            place(p, id, parent, before)?;
        }
        Command::DeleteSubtree { id } => {
            let parent = p
                .tasks
                .iter()
                .find(|t| t.id == id)
                .ok_or("Unknown task")?
                .parent;
            let ids = subtree(p, id)?;
            management::clean_task(p, &ids);
            p.tasks.retain(|t| !ids.contains(&t.id));
            p.assignments.retain(|a| !ids.contains(&a.task));
            p.links
                .retain(|l| !ids.contains(&l.from) && !ids.contains(&l.to));
            let remaining = siblings(p, parent, None);
            set_order(p, &remaining);
        }
        Command::SetPredecessors { id, links } => {
            if !p.tasks.iter().any(|t| t.id == id) {
                return Err(format!("Unknown task {id}"));
            }
            if links.iter().any(|l| l.to != id) {
                return Err("Predecessor link target does not match task".into());
            }
            p.links.retain(|l| l.to != id);
            p.links.extend(links);
        }
        Command::ProjectSettings {
            title,
            start,
            holidays,
        } => {
            p.title = title;
            p.calendar.start = start;
            p.calendar.holidays = holidays;
        }
        Command::CalendarSettings {
            start_minute,
            utc_offset,
            weekly,
            exceptions,
        } => {
            p.calendar.start_minute = start_minute;
            p.calendar.utc_offset = utc_offset;
            p.calendar.weekly = weekly;
            p.calendar.exceptions = exceptions;
        }
        Command::Dates {
            id,
            constraint,
            deadline,
        } => {
            let task = task_mut(p, id)?;
            task.constraint = constraint;
            task.deadline = deadline;
        }
        Command::AddResource(resource) => {
            if !used_resources.insert(resource.id) {
                return Err("Resource ID was already used; allocate next_resource_id".into());
            }
            p.resources.push(resource);
        }
        Command::UpdateResource(resource) => {
            let target = p
                .resources
                .iter_mut()
                .find(|r| r.id == resource.id)
                .ok_or("Unknown resource")?;
            *target = resource;
        }
        Command::DeleteResource(id) => {
            if !p.resources.iter().any(|r| r.id == id) {
                return Err("Unknown resource".into());
            }
            management::clean_resource(p, id);
            p.resources.retain(|r| r.id != id);
            p.assignments.retain(|a| a.resource != id);
        }
        Command::SetAssignments { task, assignments } => {
            if !p.tasks.iter().any(|t| t.id == task) {
                return Err("Unknown assignment task".into());
            }
            if assignments.iter().any(|a| a.task != task) {
                return Err("Assignment task does not match target".into());
            }
            let previous_work = management::work(p, task);
            p.assignments.retain(|a| a.task != task);
            p.assignments.extend(assignments);
            management::assignment_changed(p, task, previous_work)?;
        }
        Command::Policy { id, mut policy } => {
            if policy.mode == management::TaskMode::FixedWork && policy.target_work_basis.is_none()
            {
                policy.target_work_basis = Some(management::work(p, id));
            }
            p.management.policies.insert(id, policy);
        }
        Command::Work { id, target_basis } => management::set_work(p, id, target_basis)?,
        Command::PersonalCalendar { id, calendar } => {
            if let Some(calendar) = calendar {
                p.management.resource_calendars.insert(id, calendar);
            } else {
                p.management.resource_calendars.remove(&id);
            }
        }
        Command::Rate { id, cents_per_hour } => {
            p.management.hourly_rates_cents.insert(id, cents_per_hour);
        }
        Command::FixedCost { id, cents } => {
            p.management.fixed_costs_cents.insert(id, cents);
        }
        Command::Actual { id, actual } => {
            p.management.actuals.insert(id, actual);
        }
        Command::Baseline { name } => management::capture(p, name)?,
        Command::StatusDate(date) => p.management.status_date = date,
        Command::Replace(project) => {
            used_resources.extend(project.resources.iter().map(|r| r.id));
            used.extend(project.tasks.iter().map(|t| t.id));
            *p = *project;
        }
    }
    Ok(())
}
