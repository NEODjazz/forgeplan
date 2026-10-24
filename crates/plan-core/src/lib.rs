//! Deterministic working-minute scheduling and transactional editing.
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;
use std::str::FromStr;

pub const MAX_TASKS: usize = 2000;
pub const MAX_HORIZON: i64 = 48_000_000;
pub mod calendar;
pub use calendar::{Calendar, Moment, Timeline, WorkInterval};
pub const MAX_DEPTH: usize = 64;
pub type Result<T> = std::result::Result<T, String>;
pub mod exchange;
pub mod json_format;
pub mod leveling;
pub mod management;
pub use management::Management;
pub mod resources;
pub use resources::{Assignment, Availability, Resource, ResourceId, ResourceKind, Units};
pub mod editor;
pub use editor::{Command, Editor, Revision};

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(try_from = "crate::json_format::IdWire", into = "u32")]
pub struct TaskId(u32);
impl TaskId {
    pub fn new(value: u32) -> Result<Self> {
        if value == 0 {
            Err("Task ID must be positive".into())
        } else {
            Ok(Self(value))
        }
    }
    pub fn get(self) -> u32 {
        self.0
    }
}
impl fmt::Display for TaskId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl FromStr for TaskId {
    type Err = String;
    fn from_str(s: &str) -> Result<Self> {
        Self::new(s.parse().map_err(|_| "Invalid task ID")?)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "i64", into = "i64")]
pub struct WorkDuration(i64);
impl WorkDuration {
    pub fn new(minutes: i64) -> Result<Self> {
        if (0..=MAX_HORIZON).contains(&minutes) {
            Ok(Self(minutes))
        } else {
            Err("Invalid duration".into())
        }
    }
    pub fn minutes(self) -> i64 {
        self.0
    }
}
impl fmt::Display for WorkDuration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl FromStr for WorkDuration {
    type Err = String;
    fn from_str(s: &str) -> Result<Self> {
        Self::new(s.parse().map_err(|_| "Invalid duration")?)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "i64", into = "i64")]
pub struct Lag(i64);
impl Lag {
    pub fn new(minutes: i64) -> Result<Self> {
        if (-MAX_HORIZON..=MAX_HORIZON).contains(&minutes) {
            Ok(Self(minutes))
        } else {
            Err("Invalid lag".into())
        }
    }
    pub fn minutes(self) -> i64 {
        self.0
    }
}
impl fmt::Display for Lag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl FromStr for Lag {
    type Err = String;
    fn from_str(s: &str) -> Result<Self> {
        Self::new(s.parse().map_err(|_| "Invalid lag")?)
    }
}
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(try_from = "String", into = "String")]
pub struct Date(i64);
impl Date {
    pub fn parse(s: &str) -> Result<Self> {
        let b = s.as_bytes();
        if b.len() != 10
            || b[4] != b'-'
            || b[7] != b'-'
            || b.iter()
                .enumerate()
                .any(|(i, c)| i != 4 && i != 7 && !c.is_ascii_digit())
        {
            return Err(format!("Invalid date: {s}; expected YYYY-MM-DD"));
        }
        let y: i64 = s[0..4].parse().map_err(|_| "Invalid year")?;
        let m: i64 = s[5..7].parse().map_err(|_| "Invalid month")?;
        let d: i64 = s[8..10].parse().map_err(|_| "Invalid day")?;
        let leap = y % 4 == 0 && (y % 100 != 0 || y % 400 == 0);
        let lengths = [
            31,
            if leap { 29 } else { 28 },
            31,
            30,
            31,
            30,
            31,
            31,
            30,
            31,
            30,
            31,
        ];
        if !(1970..=9999).contains(&y)
            || !(1..=12).contains(&m)
            || d < 1
            || d > lengths[(m - 1) as usize]
        {
            return Err(format!("Invalid date: {s}"));
        }
        // Gregorian civil date to days since 1970-01-01.
        let y = y - i64::from(m <= 2);
        let era = y.div_euclid(400);
        let yo = y - era * 400;
        let mp = m + if m > 2 { -3 } else { 9 };
        let doy = (153 * mp + 2) / 5 + d - 1;
        Ok(Self(
            era * 146097 + yo * 365 + yo / 4 - yo / 100 + doy - 719468,
        ))
    }
    pub fn iso(self) -> String {
        let z = self.0 + 719468;
        let era = z.div_euclid(146097);
        let doe = z - era * 146097;
        let yo = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
        let mut y = yo + era * 400;
        let doy = doe - (365 * yo + yo / 4 - yo / 100);
        let mp = (5 * doy + 2) / 153;
        let d = doy - (153 * mp + 2) / 5 + 1;
        let m = mp + if mp < 10 { 3 } else { -9 };
        y += i64::from(m <= 2);
        format!("{y:04}-{m:02}-{d:02}")
    }
    fn weekday(self) -> usize {
        (self.0 + 3).rem_euclid(7) as usize
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Relation {
    FS,
    SS,
    FF,
    SF,
}
impl Relation {
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "FS" => Ok(Self::FS),
            "SS" => Ok(Self::SS),
            "FF" => Ok(Self::FF),
            "SF" => Ok(Self::SF),
            _ => Err(format!("Unknown relation: {s}")),
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::FS => "FS",
            Self::SS => "SS",
            Self::FF => "FF",
            Self::SF => "SF",
        }
    }
    #[cfg(test)]
    fn weight(self, predecessor: i64, successor: i64, lag: i64) -> i64 {
        match self {
            Self::FS => predecessor + lag,
            Self::SS => lag,
            Self::FF => predecessor - successor + lag,
            Self::SF => lag - successor,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ConstraintType {
    SNET,
    SNLT,
    FNET,
    FNLT,
    MSO,
    MFO,
}
impl ConstraintType {
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "SNET" => Ok(Self::SNET),
            "SNLT" => Ok(Self::SNLT),
            "FNET" => Ok(Self::FNET),
            "FNLT" => Ok(Self::FNLT),
            "MSO" => Ok(Self::MSO),
            "MFO" => Ok(Self::MFO),
            _ => Err("Unknown date constraint".into()),
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::SNET => "SNET",
            Self::SNLT => "SNLT",
            Self::FNET => "FNET",
            Self::FNLT => "FNLT",
            Self::MSO => "MSO",
            Self::MFO => "MFO",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Constraint {
    pub kind: ConstraintType,
    pub at: Moment,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskKind {
    Task,
    Summary,
}
impl TaskKind {
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "task" => Ok(Self::Task),
            "summary" => Ok(Self::Summary),
            _ => Err(format!("Unknown task kind: {s}")),
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Task => "task",
            Self::Summary => "summary",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Task {
    pub id: TaskId,
    pub title: String,
    pub duration: WorkDuration,
    pub parent: Option<TaskId>,
    pub order: u32,
    pub kind: TaskKind,
    pub constraint: Option<Constraint>,
    pub deadline: Option<Moment>,
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Dependency {
    pub from: TaskId,
    pub to: TaskId,
    pub relation: Relation,
    pub lag: Lag,
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Project {
    pub title: String,
    pub calendar: Calendar,
    pub tasks: Vec<Task>,
    pub links: Vec<Dependency>,
    pub resources: Vec<Resource>,
    pub assignments: Vec<Assignment>,
    #[serde(default)]
    pub management: Management,
}
#[derive(Clone, Debug)]
pub struct OutlineRow {
    pub index: usize,
    pub depth: usize,
    pub wbs: String,
}

/// Stable preorder from parent + sibling order; file row order is not authoritative.
pub fn outline(p: &Project) -> Result<Vec<OutlineRow>> {
    if std::iter::once(&p.title)
        .chain(p.tasks.iter().map(|t| &t.title))
        .any(|s| s.contains(['\t', '\r', '\n']))
    {
        return Err("Names cannot contain tabs or newlines".into());
    }
    if p.tasks.len() > MAX_TASKS {
        return Err(format!("Maximum {MAX_TASKS} tasks supported"));
    }
    let mut index = BTreeMap::new();
    let mut siblings = BTreeMap::<Option<TaskId>, Vec<usize>>::new();
    let mut positions = BTreeSet::new();
    for (i, t) in p.tasks.iter().enumerate() {
        if index.insert(t.id, i).is_some() {
            return Err(format!("Duplicate task ID: {}", t.id));
        }
        if !positions.insert((t.parent, t.order)) {
            return Err("Duplicate sibling order".into());
        }
        if t.kind == TaskKind::Summary
            && (t.duration.minutes() != 0 || t.constraint.is_some() || t.deadline.is_some())
        {
            return Err("Summary duration must be zero and dates automatic".into());
        }
        siblings.entry(t.parent).or_default().push(i);
    }
    for t in &p.tasks {
        if let Some(parent) = t.parent {
            let &i = index
                .get(&parent)
                .ok_or_else(|| format!("Unknown parent {parent}"))?;
            if p.tasks[i].kind != TaskKind::Summary {
                return Err(format!("Parent {parent} must be a summary task"));
            }
        }
        let mut at = t.parent;
        let mut seen = BTreeSet::from([t.id]);
        while let Some(id) = at {
            if !seen.insert(id) {
                return Err("Hierarchy cycle detected".into());
            }
            if seen.len() > MAX_DEPTH {
                return Err(format!("Maximum hierarchy depth is {MAX_DEPTH}"));
            }
            at = p.tasks[*index.get(&id).ok_or("Unknown parent")?].parent;
        }
    }
    for v in siblings.values_mut() {
        v.sort_by_key(|&i| p.tasks[i].order);
    }
    let mut stack = Vec::new();
    if let Some(roots) = siblings.get(&None) {
        for (pos, &i) in roots.iter().enumerate().rev() {
            stack.push((i, 0, (pos + 1).to_string()));
        }
    }
    let mut rows = Vec::new();
    while let Some((i, depth, wbs)) = stack.pop() {
        if let Some(children) = siblings.get(&Some(p.tasks[i].id)) {
            for (pos, &j) in children.iter().enumerate().rev() {
                stack.push((j, depth + 1, format!("{wbs}.{}", pos + 1)));
            }
        }
        rows.push(OutlineRow {
            index: i,
            depth,
            wbs,
        });
    }
    if rows.len() != p.tasks.len() {
        return Err("Unreachable hierarchy nodes".into());
    }
    Ok(rows)
}
#[derive(Clone, Debug)]
pub struct ScheduledTask {
    pub id: TaskId,
    pub early_start: i64,
    pub early_finish: i64,
    pub late_start: Option<i64>,
    pub late_finish: Option<i64>,
    pub total_float: Option<i64>,
    pub summary: bool,
    pub start: Moment,
    pub finish: Moment,
    pub reasons: Vec<String>,
    pub deadline_missed: bool,
}
impl ScheduledTask {
    pub fn critical(&self) -> bool {
        !self.summary && self.total_float == Some(0)
    }
}
#[derive(Clone, Debug)]
pub struct Schedule {
    pub tasks: Vec<ScheduledTask>,
    pub finish: i64,
}

// Monotone edge function. Wall-clock endpoint comparisons preserve gaps at work boundaries.
fn required_start(
    tl: &mut Timeline,
    pd: i64,
    sd: i64,
    relation: Relation,
    lag: i64,
    ps: i64,
) -> Result<i64> {
    let finish_anchor = matches!(relation, Relation::FS | Relation::FF);
    let coordinate = ps + if finish_anchor { pd } else { 0 };
    let target = coordinate + lag;
    if target < 0 {
        return Ok(-MAX_HORIZON);
    }
    if target > MAX_HORIZON {
        return Ok(MAX_HORIZON + 1);
    }
    let at = if lag > 0 {
        tl.finish_at(target)?
    } else if lag < 0 {
        tl.start_at(target)?
    } else if finish_anchor && pd > 0 {
        tl.finish_at(coordinate)?
    } else {
        tl.start_at(coordinate)?
    };
    if matches!(relation, Relation::FS | Relation::SS) || sd == 0 {
        tl.start_ceil(at)
    } else {
        Ok(tl.finish_ceil(at)? - sd)
    }
}
fn date_bounds(tl: &mut Timeline, t: &Task) -> Result<(i64, i64)> {
    let Some(c) = t.constraint else {
        return Ok((0, MAX_HORIZON));
    };
    use ConstraintType::*;
    let (lo, hi) = match c.kind {
        SNET => (tl.start_ceil(c.at)?, MAX_HORIZON),
        SNLT => (0, tl.start_floor(c.at)?),
        FNET => (
            if t.duration.minutes() == 0 {
                tl.start_ceil(c.at)?
            } else {
                tl.finish_ceil(c.at)? - t.duration.minutes()
            },
            MAX_HORIZON,
        ),
        FNLT => (
            0,
            if t.duration.minutes() == 0 {
                tl.start_floor(c.at)?
            } else {
                tl.finish_floor(c.at)? - t.duration.minutes()
            },
        ),
        MSO => {
            let n = tl.start_ceil(c.at)?;
            if tl.start_at(n)? != c.at {
                return Err(format!(
                    "Task {} MSO falls outside a working start instant",
                    t.id
                ));
            }
            (n, n)
        }
        MFO => {
            let n = if t.duration.minutes() == 0 {
                tl.start_ceil(c.at)?
            } else {
                tl.finish_ceil(c.at)?
            };
            let actual = if t.duration.minutes() == 0 {
                tl.start_at(n)?
            } else {
                tl.finish_at(n)?
            };
            if actual != c.at {
                return Err(format!(
                    "Task {} MFO falls outside a working finish instant",
                    t.id
                ));
            }
            (n - t.duration.minutes(), n - t.duration.minutes())
        }
    };
    Ok((lo.max(0), hi))
}
pub fn schedule(p: &Project) -> Result<Schedule> {
    let rows = outline(p)?;
    resources::validate(p)?;
    management::validate(p)?;
    let n = p.tasks.len();
    let mut tl = Timeline::new(&p.calendar)?;
    if p.links.len() > 20_000 {
        return Err("Maximum 20000 dependencies supported".into());
    }
    let index: BTreeMap<_, _> = p.tasks.iter().enumerate().map(|(i, t)| (t.id, i)).collect();
    let mut outgoing = vec![Vec::<(usize, Relation, i64)>::new(); n];
    let mut degree = vec![0; n];
    for l in &p.links {
        let &a = index
            .get(&l.from)
            .ok_or_else(|| format!("Unknown predecessor {}", l.from))?;
        let &b = index
            .get(&l.to)
            .ok_or_else(|| format!("Unknown successor {}", l.to))?;
        if a == b {
            return Err("Self dependency is not allowed".into());
        }
        if p.tasks[a].kind == TaskKind::Summary || p.tasks[b].kind == TaskKind::Summary {
            return Err("Links to summary tasks are not supported; link leaf tasks".into());
        }
        outgoing[a].push((b, l.relation, l.lag.minutes()));
        degree[b] += 1;
    }
    let mut early = Vec::new();
    let mut upper = Vec::new();
    for t in &p.tasks {
        let (lo, hi) = date_bounds(&mut tl, t)?;
        early.push(lo);
        upper.push(hi);
    }
    let mut queue: VecDeque<_> = (0..n).filter(|&i| degree[i] == 0).collect();
    let mut order = Vec::new();
    while let Some(a) = queue.pop_front() {
        order.push(a);
        for &(b, relation, lag) in &outgoing[a] {
            let required = required_start(
                &mut tl,
                p.tasks[a].duration.minutes(),
                p.tasks[b].duration.minutes(),
                relation,
                lag,
                early[a],
            )?;
            early[b] = early[b].max(required);
            degree[b] -= 1;
            if degree[b] == 0 {
                queue.push_back(b);
            }
        }
    }
    if order.len() != n {
        return Err("Dependency cycle detected; schedule was not changed".into());
    }
    for (i, t) in p.tasks.iter().enumerate() {
        if early[i] > upper[i] {
            return Err(format!("Task {} cannot meet {} at {}: dependencies/calendar require a later start; no changes applied",t.id,t.constraint.map(|c|c.kind.label()).unwrap_or("date constraint"),t.constraint.map(|c|c.at.local()).unwrap_or_default()));
        }
    }
    let finish = (0..n)
        .filter(|&i| p.tasks[i].kind == TaskKind::Task)
        .map(|i| early[i] + p.tasks[i].duration.minutes())
        .max()
        .unwrap_or(0);
    if finish > MAX_HORIZON {
        return Err("Schedule exceeds working-minute horizon".into());
    }
    let mut late: Vec<_> = p
        .tasks
        .iter()
        .enumerate()
        .map(|(i, t)| (finish - t.duration.minutes()).min(upper[i]))
        .collect();
    // Invert each monotone edge by binary search instead of assuming an affine weight across nonwork gaps.
    for &a in order.iter().rev() {
        for &(b, relation, lag) in &outgoing[a] {
            let (mut lo, mut hi) = (early[a], late[a]);
            while lo < hi {
                let mid = lo + (hi - lo + 1) / 2;
                let needed = required_start(
                    &mut tl,
                    p.tasks[a].duration.minutes(),
                    p.tasks[b].duration.minutes(),
                    relation,
                    lag,
                    mid,
                )?;
                if needed <= late[b] {
                    lo = mid;
                } else {
                    hi = mid - 1;
                }
            }
            late[a] = lo;
        }
    }
    let mut tasks = Vec::new();
    for (i, t) in p.tasks.iter().enumerate() {
        let start = tl.start_at(early[i])?;
        let end = if t.duration.minutes() == 0 {
            start
        } else {
            tl.finish_at(early[i] + t.duration.minutes())?
        };
        let mut reasons = Vec::new();
        if early[i] == 0 {
            reasons.push("Начало проекта и ближайший рабочий интервал".into());
        }
        if let Some(c) = t.constraint {
            reasons.push(format!("Ограничение {}: {}", c.kind.label(), c.at.local()));
        }
        if end.0 - start.0 > t.duration.minutes() {
            reasons
                .push("Работа проходит через перерывы, выходные или календарные исключения".into());
        }
        let deadline_missed = t.deadline.is_some_and(|deadline| end > deadline);
        if deadline_missed {
            reasons.push(format!(
                "Дедлайн {} превышен; он не изменяет расписание",
                t.deadline.unwrap().local()
            ));
        }
        tasks.push(ScheduledTask {
            id: t.id,
            early_start: early[i],
            early_finish: early[i] + t.duration.minutes(),
            late_start: Some(late[i]),
            late_finish: Some(late[i] + t.duration.minutes()),
            total_float: Some(late[i] - early[i]),
            summary: t.kind == TaskKind::Summary,
            start,
            finish: end,
            reasons,
            deadline_missed,
        });
    }
    for (a, edges) in outgoing.iter().enumerate() {
        for &(b, relation, lag) in edges {
            let needed = required_start(
                &mut tl,
                p.tasks[a].duration.minutes(),
                p.tasks[b].duration.minutes(),
                relation,
                lag,
                early[a],
            )?;
            if needed == early[b] {
                tasks[b].reasons.push(format!(
                    "Определяющая связь: задача {} {} {:+} рабочих мин.",
                    p.tasks[a].id,
                    relation.label(),
                    lag
                ));
            }
        }
    }
    let mut spans: Vec<Option<(i64, i64, Moment, Moment)>> = p
        .tasks
        .iter()
        .enumerate()
        .map(|(i, t)| {
            if t.kind == TaskKind::Task {
                Some((
                    tasks[i].early_start,
                    tasks[i].early_finish,
                    tasks[i].start,
                    tasks[i].finish,
                ))
            } else {
                None
            }
        })
        .collect();
    for row in rows.iter().rev() {
        let i = row.index;
        if p.tasks[i].kind == TaskKind::Summary {
            let default = tl.start_at(0)?;
            let (a, b, start, end) = spans[i].unwrap_or((0, 0, default, default));
            tasks[i].early_start = a;
            tasks[i].early_finish = b;
            tasks[i].start = start;
            tasks[i].finish = end;
            tasks[i].late_start = None;
            tasks[i].late_finish = None;
            tasks[i].total_float = None;
            tasks[i].deadline_missed = false;
            tasks[i].reasons = vec!["Даты рассчитаны по диапазону дочерних задач и вех".into()];
        }
        if let (Some(parent), Some((a, b, start, end))) = (p.tasks[i].parent, spans[i]) {
            let j = index[&parent];
            spans[j] = Some(match spans[j] {
                None => (a, b, start, end),
                Some((x, y, s, e)) => (a.min(x), b.max(y), start.min(s), end.max(e)),
            });
        }
    }
    Ok(Schedule { tasks, finish })
}

/// v1/v2 day values migrate using exactly 480 working minutes per legacy day.
pub fn parse(text: &str) -> Result<Project> {
    if text.trim_start().starts_with('{') {
        json_format::decode(text)
    } else {
        parse_legacy(text)
    }
}
pub fn parse_legacy(text: &str) -> Result<Project> {
    if text.len() > 1_000_000 {
        return Err("Project exceeds 1 MB limit".into());
    }
    let mut lines = text.lines();
    let version = match lines.next() {
        Some("forgeplan\t1") => 1,
        Some("forgeplan\t2") => 2,
        Some("forgeplan\t3") => 3,
        Some("forgeplan\t4") => 4,
        _ => return Err("Expected forgeplan<TAB>1, 2, 3 or 4 header".into()),
    };
    let mut title = None;
    let mut calendar = None;
    let mut holidays = BTreeSet::new();
    let mut tasks = Vec::new();
    let mut links = Vec::new();
    let mut resources = Vec::new();
    let mut assignments = Vec::new();
    let mut availability = BTreeMap::<ResourceId, Vec<Availability>>::new();
    let mut weekly = BTreeMap::new();
    let mut exceptions = BTreeMap::new();
    let mut settings = None;
    let mut constraints = BTreeMap::new();
    let mut deadlines = BTreeMap::new();
    let value = |s: &str| -> Result<i64> {
        let n: i64 = s.parse().map_err(|_| "Invalid minute/day value")?;
        if version < 3 {
            n.checked_mul(480).ok_or("Legacy value overflow".into())
        } else {
            Ok(n)
        }
    };
    for (i, line) in lines.enumerate() {
        if line.is_empty() {
            continue;
        }
        let row: Vec<_> = line.split('\t').collect();
        let err = || format!("Invalid row {}", i + 2);
        match row.as_slice() {
            ["project", name, start] if title.is_none() => {
                title = Some((*name).into());
                calendar = Some(Calendar::weekdays(Date::parse(start)?));
            }
            ["holiday", d] => {
                holidays.insert(Date::parse(d)?);
            }
            ["calendar", start, offset] if version >= 3 && settings.is_none() => {
                settings = Some((
                    calendar::parse_time(start, false)?,
                    calendar::parse_offset(offset)?,
                ));
            }
            ["week", day, hours] if version >= 3 => {
                let day: usize = day.parse().map_err(|_| err())?;
                if day > 6 || weekly.insert(day, calendar::intervals(hours)?).is_some() {
                    return Err("Invalid or duplicate weekday".into());
                }
            }
            ["exception", day, hours] if version >= 3 => {
                if exceptions
                    .insert(Date::parse(day)?, calendar::intervals(hours)?)
                    .is_some()
                {
                    return Err("Duplicate calendar exception".into());
                }
            }
            ["constraint", id, kind, at] if version >= 3 => {
                if constraints
                    .insert(
                        id.parse::<TaskId>()?,
                        Constraint {
                            kind: ConstraintType::parse(kind)?,
                            at: Moment::parse(at)?,
                        },
                    )
                    .is_some()
                {
                    return Err("Duplicate task constraint".into());
                }
            }
            ["deadline", id, at] if version >= 3 => {
                if deadlines
                    .insert(id.parse::<TaskId>()?, Moment::parse(at)?)
                    .is_some()
                {
                    return Err("Duplicate task deadline".into());
                }
            }
            ["task", id, duration, name] if version == 1 => tasks.push(Task {
                id: id.parse()?,
                title: (*name).into(),
                duration: WorkDuration::new(value(duration)?)?,
                parent: None,
                order: tasks.len() as u32,
                kind: TaskKind::Task,
                constraint: None,
                deadline: None,
            }),
            ["task", id, parent, order, kind, duration, name] if version >= 2 => tasks.push(Task {
                id: id.parse()?,
                title: (*name).into(),
                duration: WorkDuration::new(value(duration)?)?,
                parent: parse_parent(parent)?,
                order: order.parse().map_err(|_| err())?,
                kind: TaskKind::parse(kind)?,
                constraint: None,
                deadline: None,
            }),
            ["link", from, to, relation, lag] => links.push(Dependency {
                from: from.parse()?,
                to: to.parse()?,
                relation: Relation::parse(relation)?,
                lag: Lag::new(value(lag)?)?,
            }),
            ["resource", id, kind, capacity, name] if version == 4 => resources.push(Resource {
                id: id.parse()?,
                kind: ResourceKind::parse(kind)?,
                capacity: capacity.parse()?,
                name: (*name).into(),
                availability: vec![],
            }),
            ["availability", id, from, through, capacity] if version == 4 => availability
                .entry(id.parse()?)
                .or_default()
                .push(Availability {
                    from: Date::parse(from)?,
                    through: Date::parse(through)?,
                    capacity: capacity.parse()?,
                }),
            ["assignment", task, resource, units] if version == 4 => assignments.push(Assignment {
                task: task.parse()?,
                resource: resource.parse()?,
                units: units.parse()?,
            }),
            _ => return Err(err()),
        }
    }
    let mut calendar = calendar.ok_or("Missing project row")?;
    calendar.holidays = holidays;
    calendar.exceptions = exceptions;
    if let Some((start, offset)) = settings {
        calendar.start_minute = start;
        calendar.utc_offset = offset;
    }
    if !weekly.is_empty() {
        if weekly.len() != 7 {
            return Err("Provide all 7 weekday rows".into());
        }
        for (i, hours) in weekly {
            calendar.weekly[i] = hours;
        }
    }
    for t in &mut tasks {
        t.constraint = constraints.remove(&t.id);
        t.deadline = deadlines.remove(&t.id);
    }
    if !constraints.is_empty() || !deadlines.is_empty() {
        return Err("Date rule references unknown task".into());
    }
    for resource in &mut resources {
        resource.availability = availability.remove(&resource.id).unwrap_or_default();
    }
    if !availability.is_empty() {
        return Err("Availability references unknown resource".into());
    }
    let p = Project {
        title: title.unwrap(),
        calendar,
        tasks,
        links,
        resources,
        assignments,
        management: Management::default(),
    };
    schedule(&p)?;
    Ok(p)
}
pub fn parse_parent(s: &str) -> Result<Option<TaskId>> {
    if s == "-" {
        Ok(None)
    } else {
        Ok(Some(s.parse()?))
    }
}
pub fn serialize(p: &Project) -> Result<String> {
    json_format::encode(p)
}
pub fn serialize_legacy(p: &Project) -> Result<String> {
    for s in std::iter::once(&p.title).chain(p.tasks.iter().map(|t| &t.title)) {
        if s.contains(['\t', '\r', '\n']) {
            return Err("Names cannot contain tabs or newlines".into());
        }
    }
    schedule(p)?;
    let mut out = format!(
        "forgeplan\t4\nproject\t{}\t{}\ncalendar\t{}\t{}\n",
        p.title,
        p.calendar.start.iso(),
        calendar::time_text(p.calendar.start_minute),
        calendar::offset_text(p.calendar.utc_offset)
    );
    for (i, hours) in p.calendar.weekly.iter().enumerate() {
        out.push_str(&format!("week\t{i}\t{}\n", calendar::intervals_text(hours)));
    }
    for d in &p.calendar.holidays {
        out.push_str(&format!("holiday\t{}\n", d.iso()));
    }
    for (d, hours) in &p.calendar.exceptions {
        out.push_str(&format!(
            "exception\t{}\t{}\n",
            d.iso(),
            calendar::intervals_text(hours)
        ));
    }
    for row in outline(p)? {
        let t = &p.tasks[row.index];
        out.push_str(&format!(
            "task\t{}\t{}\t{}\t{}\t{}\t{}\n",
            t.id,
            t.parent.map(|id| id.to_string()).unwrap_or("-".into()),
            t.order,
            t.kind.label(),
            t.duration,
            t.title
        ));
        if let Some(c) = t.constraint {
            out.push_str(&format!(
                "constraint\t{}\t{}\t{}\n",
                t.id,
                c.kind.label(),
                c.at.local()
            ));
        }
        if let Some(d) = t.deadline {
            out.push_str(&format!("deadline\t{}\t{}\n", t.id, d.local()));
        }
    }
    for l in &p.links {
        out.push_str(&format!(
            "link\t{}\t{}\t{}\t{}\n",
            l.from,
            l.to,
            l.relation.label(),
            l.lag
        ));
    }
    let mut resource_rows: Vec<_> = p.resources.iter().collect();
    resource_rows.sort_by_key(|r| r.id);
    for resource in resource_rows {
        out.push_str(&format!(
            "resource\t{}\t{}\t{}\t{}\n",
            resource.id,
            resource.kind.label(),
            resource.capacity,
            resource.name
        ));
        let mut periods: Vec<_> = resource.availability.iter().collect();
        periods.sort_by_key(|a| a.from);
        for a in periods {
            out.push_str(&format!(
                "availability\t{}\t{}\t{}\t{}\n",
                resource.id,
                a.from.iso(),
                a.through.iso(),
                a.capacity
            ));
        }
    }
    let mut assignments: Vec<_> = p.assignments.iter().collect();
    assignments.sort_by_key(|a| (a.task, a.resource));
    for a in assignments {
        out.push_str(&format!(
            "assignment\t{}\t{}\t{}\n",
            a.task, a.resource, a.units
        ));
    }
    if out.len() > 1_000_000 {
        return Err("Project exceeds 1 MB limit".into());
    }
    Ok(out)
}
pub fn json_string(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c < ' ' => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn opt_number(n: Option<i64>) -> String {
    n.map(|n| n.to_string()).unwrap_or("null".into())
}
fn optional_moment(m: Option<Moment>) -> String {
    m.map(|m| json_string(&m.local())).unwrap_or("null".into())
}
pub fn schedule_json(p: &Project, s: &Schedule) -> String {
    let load = resources::analyze(p, s).expect("validated resource model");
    let rows:Vec<_>=outline(p).expect("validated schedule").iter().map(|row|{
  let t=&p.tasks[row.index];let r=&s.tasks[row.index];
  format!("{{\"work_minutes\":{},\"id\":{},\"parent\":{},\"order\":{},\"kind\":{},\"depth\":{},\"wbs\":{},\"title\":{},\"duration\":{},\"start\":{},\"finish\":{},\"es\":{},\"ef\":{},\"ls\":{},\"lf\":{},\"float\":{},\"critical\":{},\"constraint_type\":{},\"constraint_at\":{},\"deadline\":{},\"deadline_missed\":{},\"reasons\":[{}]}}",resources::work_decimal(*load.task_work_basis.get(&t.id).unwrap_or(&0)),t.id,t.parent.map(|id|id.to_string()).unwrap_or("null".into()),t.order,json_string(t.kind.label()),row.depth,json_string(&row.wbs),json_string(&t.title),r.early_finish-r.early_start,json_string(&r.start.iso(p.calendar.utc_offset)),json_string(&r.finish.iso(p.calendar.utc_offset)),r.early_start,r.early_finish,opt_number(r.late_start),opt_number(r.late_finish),opt_number(r.total_float),r.critical(),t.constraint.map(|c|json_string(c.kind.label())).unwrap_or("null".into()),optional_moment(t.constraint.map(|c|c.at)),optional_moment(t.deadline),r.deadline_missed,r.reasons.iter().map(|reason|json_string(reason)).collect::<Vec<_>>().join(","))
 }).collect();
    let weekly = p
        .calendar
        .weekly
        .iter()
        .map(|hours| json_string(&calendar::intervals_text(hours)))
        .collect::<Vec<_>>()
        .join(",");
    let exceptions = p
        .calendar
        .exceptions
        .iter()
        .map(|(d, h)| {
            format!(
                "{{\"date\":{},\"hours\":{}}}",
                json_string(&d.iso()),
                json_string(&calendar::intervals_text(h))
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let output=format!("{{\"title\":{},\"unit\":\"working_minutes\",\"working_minutes\":{},\"working_days\":{},\"calendar\":{{\"start_date\":{},\"start_time\":{},\"offset\":{},\"weekly\":[{}],\"holidays\":[{}],\"exceptions\":[{}]}},\"tasks\":[{}]}}",json_string(&p.title),s.finish,s.finish as f64/480.0,json_string(&p.calendar.start.iso()),json_string(&calendar::time_text(p.calendar.start_minute)),json_string(&calendar::offset_text(p.calendar.utc_offset)),weekly,p.calendar.holidays.iter().map(|d|json_string(&d.iso())).collect::<Vec<_>>().join(","),exceptions,rows.join(","));
    let mut output: serde_json::Value = serde_json::from_str(&output).expect("valid schedule JSON");
    output["resource_report"] =
        serde_json::from_str(&resources::json(p, &load)).expect("valid resource JSON");
    output["management"] = management::report(p, s, &load).expect("validated management model");
    output["links"] = serde_json::to_value(&p.links).expect("serializable links");
    serde_json::to_string(&output).expect("serializable report")
}
impl TryFrom<u32> for TaskId {
    type Error = String;
    fn try_from(value: u32) -> Result<Self> {
        Self::new(value)
    }
}
impl From<TaskId> for u32 {
    fn from(value: TaskId) -> Self {
        value.get()
    }
}

impl TryFrom<i64> for WorkDuration {
    type Error = String;
    fn try_from(value: i64) -> Result<Self> {
        Self::new(value)
    }
}
impl From<WorkDuration> for i64 {
    fn from(value: WorkDuration) -> Self {
        value.minutes()
    }
}

impl TryFrom<i64> for Lag {
    type Error = String;
    fn try_from(value: i64) -> Result<Self> {
        Self::new(value)
    }
}
impl From<Lag> for i64 {
    fn from(value: Lag) -> Self {
        value.minutes()
    }
}

impl TryFrom<String> for Date {
    type Error = String;
    fn try_from(value: String) -> Result<Self> {
        Self::parse(&value)
    }
}
impl From<Date> for String {
    fn from(value: Date) -> Self {
        value.iso()
    }
}

impl TryFrom<crate::json_format::IdWire> for TaskId {
    type Error = String;
    fn try_from(value: crate::json_format::IdWire) -> Result<Self> {
        Self::new(value.value()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn project(durations: &[i64], links: &[(u32, u32, Relation, i64)]) -> Project {
        Project {
            title: "test".into(),
            calendar: Calendar::weekdays(Date::parse("2026-10-23").unwrap()),
            tasks: durations
                .iter()
                .enumerate()
                .map(|(i, &d)| Task {
                    id: TaskId::new(i as u32 + 1).unwrap(),
                    parent: None,
                    order: i as u32,
                    kind: TaskKind::Task,
                    constraint: None,
                    deadline: None,
                    title: format!("T{i}"),
                    duration: WorkDuration::new(d).unwrap(),
                })
                .collect(),
            management: Management::default(),
            resources: vec![],
            assignments: vec![],
            links: links
                .iter()
                .map(|&(from, to, relation, lag)| Dependency {
                    from: TaskId::new(from).unwrap(),
                    to: TaskId::new(to).unwrap(),
                    relation,
                    lag: Lag::new(lag).unwrap(),
                })
                .collect(),
        }
    }
    #[test]
    fn diamond_critical_path_and_float() {
        let p = project(
            &[2, 5, 2, 1],
            &[
                (1, 2, Relation::FS, 0),
                (1, 3, Relation::FS, 0),
                (2, 4, Relation::FS, 0),
                (3, 4, Relation::FS, 0),
            ],
        );
        let s = schedule(&p).unwrap();
        assert_eq!(s.finish, 8);
        assert_eq!(
            s.tasks.iter().map(|t| t.early_start).collect::<Vec<_>>(),
            vec![0, 2, 2, 7]
        );
        assert_eq!(
            s.tasks
                .iter()
                .map(|t| t.total_float.unwrap())
                .collect::<Vec<_>>(),
            vec![0, 0, 3, 0]
        );
    }
    #[test]
    fn all_relation_types_and_signed_lags() {
        for (r, lag, expected) in [
            (Relation::FS, 0, 5),
            (Relation::SS, 2, 2),
            (Relation::FF, 1, 4),
            (Relation::SF, 4, 2),
            (Relation::FS, -2, 3),
        ] {
            let s = schedule(&project(&[5, 2], &[(1, 2, r, lag)])).unwrap();
            assert_eq!(s.tasks[1].early_start, expected);
        }
    }
    #[test]
    fn holiday_weekend_and_exclusive_finish() {
        let mut p = project(&[960, 480], &[(1, 2, Relation::FS, 0)]);
        p.calendar
            .holidays
            .insert(Date::parse("2026-10-26").unwrap());
        let s = schedule(&p).unwrap();
        assert_eq!(s.tasks[0].start.local()[..10].to_string(), "2026-10-23");
        assert_eq!(s.tasks[0].finish.local()[..10].to_string(), "2026-10-27");
        assert_eq!(s.tasks[1].start.local()[..10].to_string(), "2026-10-28");
    }
    #[test]
    fn weekend_start_and_milestone() {
        let mut p = project(&[0, 1], &[(1, 2, Relation::FS, 0)]);
        p.calendar.start = Date::parse("2026-10-24").unwrap();
        let s = schedule(&p).unwrap();
        assert_eq!(s.tasks[0].start.local()[..10].to_string(), "2026-10-26");
        assert_eq!(s.tasks[0].early_finish, 0);
    }
    #[test]
    fn rejects_invalid_graphs_and_values() {
        assert!(schedule(&project(
            &[1, 1],
            &[(1, 2, Relation::FS, 0), (2, 1, Relation::FS, 0)]
        ))
        .is_err());
        assert!(schedule(&project(&[1], &[(1, 9, Relation::FS, 0)])).is_err());
        assert!(schedule(&project(&[1], &[(1, 1, Relation::FS, 0)])).is_err());
        assert!(WorkDuration::new(-1).is_err());
        let mut p = project(&[1, 1], &[]);
        p.tasks[1].id = TaskId::new(1).unwrap();
        assert!(schedule(&p).is_err());
        p.tasks[1].id = TaskId::new(2).unwrap();
        p.calendar.weekly = std::array::from_fn(|_| vec![]);
        assert!(schedule(&p).is_err());
        assert!(schedule(&project(&[MAX_HORIZON, 1], &[(1, 2, Relation::FS, 0)])).is_err());
    }
    #[test]
    fn dates_and_leap_years() {
        for d in [
            "1970-01-01",
            "2000-02-29",
            "2026-10-23",
            "2100-03-01",
            "9999-12-31",
        ] {
            assert_eq!(Date::parse(d).unwrap().iso(), d);
        }
        for d in [
            "2026-02-29",
            "2100-02-29",
            "2026-13-01",
            "2026-00-01",
            "2026-01-00",
            "ééééé",
            "2026-2-01",
        ] {
            assert!(Date::parse(d).is_err());
        }
        assert_eq!(Date::parse("1970-01-01").unwrap().weekday(), 3);
    }
    #[test]
    fn empty_and_independent_projects() {
        assert_eq!(schedule(&project(&[], &[])).unwrap().finish, 0);
        let s = schedule(&project(&[2, 5], &[])).unwrap();
        assert_eq!(s.tasks[0].total_float, Some(3));
        assert_eq!(s.tasks[1].total_float, Some(0));
    }
    #[test]
    fn roundtrip_and_strict_format() {
        let mut p = project(&[1, 0], &[(1, 2, Relation::FS, -1)]);
        p.title = "План «А»".into();
        p.calendar
            .holidays
            .insert(Date::parse("2026-10-27").unwrap());
        let text = serialize(&p).unwrap();
        assert_eq!(serialize(&parse(&text).unwrap()).unwrap(), text);
        assert!(parse("forgeplan\t99\n").is_err());
        assert!(parse(&(text.clone() + "unknown\tx\n")).is_err());
        p.title = "bad\ttitle".into();
        assert!(serialize(&p).is_err());
    }
    #[test]
    fn enumerated_dags_obey_constraints_and_late_bounds() {
        // Enumerate mixed-relation DAGs; compare CPM to brute-force feasible starts.
        let relations = [Relation::FS, Relation::SS, Relation::FF, Relation::SF];
        for r in relations {
            for lag in -2..=2 {
                let p = project(&[2, 1, 2], &[(1, 2, r, lag), (2, 3, Relation::FS, 0)]);
                let s = schedule(&p).unwrap();
                let w = r.weight(2, 1, lag);
                let mut feasible = Vec::new();
                for a in 0..=s.finish {
                    for b in 0..=s.finish {
                        for c in 0..=s.finish {
                            if b >= a + w
                                && c > b
                                && a + 2 <= s.finish
                                && b < s.finish
                                && c + 2 <= s.finish
                            {
                                feasible.push([a, b, c]);
                            }
                        }
                    }
                }
                assert!(!feasible.is_empty());
                for i in 0..3 {
                    assert_eq!(
                        s.tasks[i].early_start,
                        feasible.iter().map(|v| v[i]).min().unwrap()
                    );
                    assert_eq!(
                        s.tasks[i].late_start.unwrap(),
                        feasible.iter().map(|v| v[i]).max().unwrap()
                    );
                }
            }
        }
    }
}
