//! Fixed-units assignment analysis. Reporting never changes the project schedule.
use crate::*;
pub const MAX_RESOURCES: usize = 200;
pub const MAX_ASSIGNMENTS: usize = 10_000;
pub const DAILY_LIMIT: usize = 90;
pub const OVERLOAD_LIMIT: usize = 200;
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(try_from = "crate::json_format::IdWire", into = "u32")]
pub struct ResourceId(u32);
impl ResourceId {
    pub fn new(id: u32) -> Result<Self> {
        if id == 0 {
            Err("Resource ID must be positive".into())
        } else {
            Ok(Self(id))
        }
    }
    pub fn get(self) -> u32 {
        self.0
    }
}
impl fmt::Display for ResourceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl FromStr for ResourceId {
    type Err = String;
    fn from_str(s: &str) -> Result<Self> {
        Self::new(s.parse().map_err(|_| "Invalid resource ID")?)
    }
}
/// 10000 basis points = 100%. Exact work uses minute*basis_points, no rounding.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "u32", into = "u32")]
pub struct Units(u32);
impl Units {
    pub fn new(basis_points: u32) -> Result<Self> {
        if basis_points <= 1_000_000 {
            Ok(Self(basis_points))
        } else {
            Err("Units exceed 10000 percent".into())
        }
    }
    pub fn basis_points(self) -> u32 {
        self.0
    }
}
impl fmt::Display for Units {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl FromStr for Units {
    type Err = String;
    fn from_str(s: &str) -> Result<Self> {
        Self::new(s.parse().map_err(|_| "Expected integer basis points")?)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceKind {
    Person,
    Generic,
}
impl ResourceKind {
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "person" => Ok(Self::Person),
            "generic" => Ok(Self::Generic),
            _ => Err("Expected person or generic resource".into()),
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Person => "person",
            Self::Generic => "generic",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Availability {
    pub from: Date,
    pub through: Date,
    pub capacity: Units,
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Resource {
    pub id: ResourceId,
    pub name: String,
    pub kind: ResourceKind,
    pub capacity: Units,
    pub availability: Vec<Availability>,
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Assignment {
    pub task: TaskId,
    pub resource: ResourceId,
    pub units: Units,
}
pub fn validate(p: &Project) -> Result<()> {
    if p.resources.len() > MAX_RESOURCES || p.assignments.len() > MAX_ASSIGNMENTS {
        return Err("Resource/assignment limit exceeded".into());
    }
    let mut ids = BTreeSet::new();
    let mut periods = 0;
    for r in &p.resources {
        if !ids.insert(r.id) {
            return Err("Duplicate resource ID".into());
        }
        if r.name.contains(['\t', '\r', '\n']) {
            return Err("Resource names cannot contain tabs or newlines".into());
        }
        periods += r.availability.len();
        if periods > 5000 {
            return Err("Maximum 5000 availability periods".into());
        }
        let mut ranges: Vec<_> = r.availability.iter().collect();
        ranges.sort_by_key(|r| r.from);
        let mut last = None;
        for a in ranges {
            if a.from > a.through || last.is_some_and(|d| d >= a.from) {
                return Err(format!(
                    "Resource {} availability dates overlap or are reversed",
                    r.id
                ));
            }
            last = Some(a.through);
        }
    }
    let tasks: BTreeMap<_, _> = p.tasks.iter().map(|t| (t.id, t)).collect();
    let mut pairs = BTreeSet::new();
    for a in &p.assignments {
        if !ids.contains(&a.resource) {
            return Err(format!("Unknown resource {}", a.resource));
        }
        let task = tasks
            .get(&a.task)
            .ok_or_else(|| format!("Assignment references unknown task {}", a.task))?;
        if task.kind == TaskKind::Summary {
            return Err("Assignments to summary tasks are not supported".into());
        }
        if a.units.basis_points() == 0 {
            return Err("Assignment units must be positive; remove the assignment instead".into());
        }
        if !pairs.insert((a.task, a.resource)) {
            return Err("Duplicate task/resource assignment".into());
        }
    }
    Ok(())
}
#[derive(Clone, Debug)]
pub struct DailyLoad {
    pub date: Date,
    pub planned_basis: i64,
    pub capacity_basis: i64,
    pub excess_basis: i64,
    pub peak_units: u32,
    pub overloaded_minutes: i64,
}
#[derive(Clone, Debug)]
pub struct Overload {
    pub start: Moment,
    pub finish: Moment,
    pub working_minutes: i64,
    pub demand_units: u32,
    pub capacity_units: u32,
    pub task_ids: Vec<TaskId>,
    pub active_task_count: usize,
}
#[derive(Clone, Debug)]
pub struct ResourceLoad {
    pub id: ResourceId,
    pub planned_basis: i64,
    pub capacity_basis: i64,
    pub excess_basis: i64,
    pub peak_units: u32,
    pub overloaded_minutes: i64,
    pub overload_count: usize,
    pub overloads: Vec<Overload>,
    pub daily: Vec<DailyLoad>,
    pub daily_truncated: bool,
}
#[derive(Clone, Debug)]
pub struct ResourceReport {
    pub resources: Vec<ResourceLoad>,
    pub task_work_basis: BTreeMap<TaskId, i64>,
}
/// Exact integer minute*basis-point value formatted as a JSON decimal minute count.
pub fn work_decimal(n: i64) -> String {
    format!(
        "{}{}.{:04}",
        if n < 0 { "-" } else { "" },
        n.unsigned_abs() / 10_000,
        n.unsigned_abs() % 10_000
    )
}
pub fn analyze(p: &Project, s: &Schedule) -> Result<ResourceReport> {
    validate(p)?;
    let scheduled: BTreeMap<_, _> = s.tasks.iter().map(|t| (t.id, t)).collect();
    if s.finish < 0
        || s.finish > MAX_HORIZON
        || scheduled.len() != p.tasks.len()
        || scheduled.len() != s.tasks.len()
        || p.tasks.iter().any(|task| {
            scheduled.get(&task.id).is_none_or(|t| {
                t.early_start < 0
                    || t.early_finish < t.early_start
                    || t.early_finish > s.finish
                    || (task.kind == TaskKind::Task
                        && t.early_finish - t.early_start != task.duration.minutes())
            })
        })
    {
        return Err("Resource analysis requires a matching validated schedule".into());
    }
    let mut tl = Timeline::new(&p.calendar)?;
    let (days, truncated) = tl.day_ranges(s.finish, DAILY_LIMIT)?;
    let mut work = BTreeMap::new();
    for a in &p.assignments {
        let task = scheduled[&a.task];
        *work.entry(a.task).or_insert(0) +=
            (task.early_finish - task.early_start) * i64::from(a.units.basis_points());
    }
    // Summary work is a sum of descendants, independent of duration/span rollups.
    for row in outline(p)?.iter().rev() {
        let t = &p.tasks[row.index];
        if let Some(parent) = t.parent {
            let own = *work.get(&t.id).unwrap_or(&0);
            *work.entry(parent).or_insert(0) += own;
        }
    }
    let mut reports = Vec::new();
    let mut resources: Vec<_> = p.resources.iter().collect();
    resources.sort_by_key(|r| r.id);
    for resource in resources {
        let mut report = ResourceLoad {
            id: resource.id,
            planned_basis: 0,
            capacity_basis: 0,
            excess_basis: 0,
            peak_units: 0,
            overloaded_minutes: 0,
            overload_count: 0,
            overloads: vec![],
            daily: days
                .iter()
                .map(|d| DailyLoad {
                    date: d.date,
                    planned_basis: 0,
                    capacity_basis: 0,
                    excess_basis: 0,
                    peak_units: 0,
                    overloaded_minutes: 0,
                })
                .collect(),
            daily_truncated: truncated,
        };
        if s.finish == 0 {
            reports.push(report);
            continue;
        }
        let mut events = BTreeMap::<i64, Vec<(TaskId, i64)>>::new();
        events.entry(0).or_default();
        events.entry(s.finish).or_default();
        for a in p.assignments.iter().filter(|a| a.resource == resource.id) {
            let task = scheduled[&a.task];
            if task.early_start < task.early_finish {
                events
                    .entry(task.early_start)
                    .or_default()
                    .push((a.task, i64::from(a.units.basis_points())));
                events
                    .entry(task.early_finish)
                    .or_default()
                    .push((a.task, -i64::from(a.units.basis_points())));
            }
        }
        let first_day = Date(tl.start_at(0)?.0.div_euclid(1440));
        let last_day = Date((tl.finish_at(s.finish)?.0 - 1).div_euclid(1440));
        let mut capacities = Vec::new();
        for a in &resource.availability {
            if a.through < first_day || a.from > last_day {
                continue;
            }
            let left = if a.from <= first_day {
                0
            } else {
                tl.start_ceil(Moment(a.from.0 * 1440))?.min(s.finish)
            };
            let right = if a.through >= last_day {
                s.finish
            } else {
                tl.start_ceil(Moment((a.through.0 + 1) * 1440))?
                    .min(s.finish)
            };
            if left < right {
                capacities.push((left, right, a.capacity.basis_points()));
                events.entry(left).or_default();
                events.entry(right).or_default();
            }
        }
        capacities.sort_by_key(|v| v.0);
        let mut personal_open = None;
        if let Some(personal) = p.management.resource_calendars.get(&resource.id) {
            let mut cal = p.calendar.clone();
            cal.weekly = personal.weekly.clone();
            cal.exceptions = personal.exceptions.clone();
            let (all_days, _) = tl.day_ranges(s.finish, usize::MAX)?;
            let mut windows = Vec::new();
            for day in all_days {
                events.entry(day.start).or_default();
                events.entry(day.end).or_default();
                for interval in cal.on(day.date) {
                    let left = tl
                        .start_ceil(Moment(day.date.0 * 1440 + i64::from(interval.start)))?
                        .max(day.start)
                        .min(day.end);
                    let right = tl
                        .start_ceil(Moment(day.date.0 * 1440 + i64::from(interval.end)))?
                        .max(day.start)
                        .min(day.end);
                    if left < right {
                        events.entry(left).or_default();
                        events.entry(right).or_default();
                        windows.push((left, right));
                    }
                }
            }
            windows.sort();
            personal_open = Some(windows);
        }
        let points: Vec<_> = events.keys().copied().collect();
        let mut active = BTreeMap::<TaskId, i64>::new();
        let mut demand = 0i64;
        let mut capacity_index = 0;
        for pair in points.windows(2) {
            let (left, right) = (pair[0], pair[1]);
            for &(task, delta) in &events[&left] {
                let value = active.entry(task).or_insert(0);
                *value += delta;
                demand += delta;
                if *value == 0 {
                    active.remove(&task);
                }
            }
            while capacity_index < capacities.len() && capacities[capacity_index].1 <= left {
                capacity_index += 1;
            }
            let mut cap = capacities
                .get(capacity_index)
                .filter(|(a, b, _)| *a <= left && left < *b)
                .map(|(_, _, c)| *c)
                .unwrap_or(resource.capacity.basis_points());
            if let Some(windows) = &personal_open {
                let i = windows.partition_point(|(_, end)| *end <= left);
                if !windows
                    .get(i)
                    .is_some_and(|(start, end)| *start <= left && left < *end)
                {
                    cap = 0;
                }
            }
            let excess = (demand - i64::from(cap)).max(0);
            let length = right - left;
            report.planned_basis += demand * length;
            report.capacity_basis += i64::from(cap) * length;
            report.excess_basis += excess * length;
            report.peak_units = report.peak_units.max(demand as u32);
            if excess > 0 {
                report.overloaded_minutes += length;
                report.overload_count += 1;
                if report.overloads.len() < OVERLOAD_LIMIT {
                    report.overloads.push(Overload {
                        start: tl.start_at(left)?,
                        finish: tl.finish_at(right)?,
                        working_minutes: length,
                        demand_units: demand as u32,
                        capacity_units: cap,
                        task_ids: active.keys().take(20).copied().collect(),
                        active_task_count: active.len(),
                    });
                }
            }
            for (day, row) in days.iter().zip(&mut report.daily) {
                let overlap = right.min(day.end) - left.max(day.start);
                if overlap > 0 {
                    row.planned_basis += demand * overlap;
                    row.capacity_basis += i64::from(cap) * overlap;
                    row.excess_basis += excess * overlap;
                    row.peak_units = row.peak_units.max(demand as u32);
                    if excess > 0 {
                        row.overloaded_minutes += overlap;
                    }
                }
            }
        }
        reports.push(report);
    }
    Ok(ResourceReport {
        resources: reports,
        task_work_basis: work,
    })
}
pub fn json(p: &Project, report: &ResourceReport) -> String {
    let rows=report.resources.iter().map(|r|{
  let resource=p.resources.iter().find(|a|a.id==r.id).expect("known resource");
  let availability=resource.availability.iter().map(|a|format!("{{\"from\":{},\"through\":{},\"capacity_bp\":{}}}",json_string(&a.from.iso()),json_string(&a.through.iso()),a.capacity)).collect::<Vec<_>>().join(",");
  let daily=r.daily.iter().map(|d|format!("{{\"date\":{},\"planned_minutes\":{},\"capacity_minutes\":{},\"excess_minutes\":{},\"peak_units_bp\":{},\"overloaded_minutes\":{}}}",json_string(&d.date.iso()),work_decimal(d.planned_basis),work_decimal(d.capacity_basis),work_decimal(d.excess_basis),d.peak_units,d.overloaded_minutes)).collect::<Vec<_>>().join(",");
  let overloads=r.overloads.iter().map(|o|format!("{{\"start\":{},\"finish\":{},\"working_minutes\":{},\"demand_bp\":{},\"capacity_bp\":{},\"active_task_count\":{},\"task_ids\":[{}]}}",json_string(&o.start.iso(p.calendar.utc_offset)),json_string(&o.finish.iso(p.calendar.utc_offset)),o.working_minutes,o.demand_units,o.capacity_units,o.active_task_count,o.task_ids.iter().map(|id|id.to_string()).collect::<Vec<_>>().join(","))).collect::<Vec<_>>().join(",");
  format!("{{\"id\":{},\"name\":{},\"kind\":{},\"capacity_bp\":{},\"availability\":[{}],\"planned_minutes\":{},\"capacity_minutes\":{},\"excess_minutes\":{},\"peak_units_bp\":{},\"overloaded_minutes\":{},\"overload_count\":{},\"overloads_truncated\":{},\"daily_truncated\":{},\"daily\":[{}],\"overloads\":[{}]}}",r.id,json_string(&resource.name),json_string(resource.kind.label()),resource.capacity,availability,work_decimal(r.planned_basis),work_decimal(r.capacity_basis),work_decimal(r.excess_basis),r.peak_units,r.overloaded_minutes,r.overload_count,r.overload_count>r.overloads.len(),r.daily_truncated,daily,overloads)
 }).collect::<Vec<_>>().join(",");
    let assignments = p
        .assignments
        .iter()
        .map(|a| {
            format!(
                "{{\"task\":{},\"resource\":{},\"units_bp\":{}}}",
                a.task, a.resource, a.units
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{{\"resources\":[{}],\"assignments\":[{}],\"daily_limit\":{},\"overload_limit\":{}}}",
        rows, assignments, DAILY_LIMIT, OVERLOAD_LIMIT
    )
}

impl TryFrom<u32> for ResourceId {
    type Error = String;
    fn try_from(value: u32) -> Result<Self> {
        Self::new(value)
    }
}
impl From<ResourceId> for u32 {
    fn from(value: ResourceId) -> Self {
        value.get()
    }
}

impl TryFrom<u32> for Units {
    type Error = String;
    fn try_from(value: u32) -> Result<Self> {
        Self::new(value)
    }
}
impl From<Units> for u32 {
    fn from(value: Units) -> Self {
        value.basis_points()
    }
}

impl TryFrom<crate::json_format::IdWire> for ResourceId {
    type Error = String;
    fn try_from(value: crate::json_format::IdWire) -> Result<Self> {
        Self::new(value.value()?)
    }
}
