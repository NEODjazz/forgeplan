use crate::*;
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskMode {
    #[default]
    FixedDuration,
    FixedWork,
    FixedUnits,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskPolicy {
    pub mode: TaskMode,
    pub effort_driven: bool,
    pub target_work_basis: Option<i64>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PersonalCalendar {
    pub weekly: [Vec<WorkInterval>; 7],
    pub exceptions: BTreeMap<Date, Vec<WorkInterval>>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Actual {
    pub percent_complete: u8,
    pub work_basis: i64,
    pub cost_cents: i64,
    pub start: Option<Moment>,
    pub finish: Option<Moment>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BaselineTask {
    pub id: TaskId,
    pub start: Moment,
    pub finish: Moment,
    pub es: i64,
    pub ef: i64,
    pub work_basis: i64,
    pub cost_cents: i64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Baseline {
    pub name: String,
    pub calendar: Calendar,
    pub tasks: Vec<BaselineTask>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Management {
    pub policies: BTreeMap<TaskId, TaskPolicy>,
    pub resource_calendars: BTreeMap<ResourceId, PersonalCalendar>,
    pub hourly_rates_cents: BTreeMap<ResourceId, i64>,
    pub fixed_costs_cents: BTreeMap<TaskId, i64>,
    pub actuals: BTreeMap<TaskId, Actual>,
    pub baselines: Vec<Baseline>,
    pub active_baseline: Option<String>,
    pub status_date: Option<Moment>,
}
pub fn work(p: &Project, id: TaskId) -> i64 {
    let duration = p
        .tasks
        .iter()
        .find(|t| t.id == id)
        .map_or(0, |t| t.duration.minutes());
    p.assignments
        .iter()
        .filter(|a| a.task == id)
        .map(|a| duration * i64::from(a.units.basis_points()))
        .sum()
}
pub fn set_units_for_work(p: &mut Project, id: TaskId, target: i64) -> Result<()> {
    let duration = p
        .tasks
        .iter()
        .find(|t| t.id == id)
        .ok_or("Unknown work task")?
        .duration
        .minutes();
    if duration == 0 {
        return if target == 0 {
            Ok(())
        } else {
            Err("Positive work requires positive duration".into())
        };
    }
    let total = (target + duration - 1) / duration;
    if total <= 0 {
        return Err("Positive-duration fixed-duration work must be positive".into());
    }
    let indices: Vec<_> = p
        .assignments
        .iter()
        .enumerate()
        .filter(|(_, a)| a.task == id)
        .map(|(i, _)| i)
        .collect();
    let sum: i64 = indices
        .iter()
        .map(|&i| i64::from(p.assignments[i].units.basis_points()))
        .sum();
    if sum == 0 {
        return Err("Work requires at least one assignment".into());
    }
    let mut values: Vec<_> = indices
        .iter()
        .map(|&i| {
            let n = i128::from(total) * i128::from(p.assignments[i].units.basis_points());
            (
                i,
                (n / i128::from(sum)) as i64,
                (n % i128::from(sum)) as i64,
            )
        })
        .collect();
    let allocated: i64 = values.iter().map(|v| v.1).sum();
    values.sort_by_key(|v| std::cmp::Reverse(v.2));
    for v in values.iter_mut().take((total - allocated) as usize) {
        v.1 += 1;
    }
    for (i, value, _) in values {
        if value <= 0 {
            return Err("Work is too small for the assignment precision".into());
        }
        p.assignments[i].units =
            Units::new(value.try_into().map_err(|_| "Assignment units overflow")?)?;
    }
    Ok(())
}
pub fn set_work(p: &mut Project, id: TaskId, target: i64) -> Result<()> {
    if !(0..=1_000_000_000_000_000_000).contains(&target) {
        return Err("Work outside supported range".into());
    }
    let mode = p
        .management
        .policies
        .get(&id)
        .map_or(TaskMode::FixedDuration, |p| p.mode);
    if mode == TaskMode::FixedDuration {
        set_units_for_work(p, id, target)?;
    } else {
        let units: i64 = p
            .assignments
            .iter()
            .filter(|a| a.task == id)
            .map(|a| i64::from(a.units.basis_points()))
            .sum();
        if units == 0 {
            return Err("Work requires at least one assignment".into());
        }
        let duration = WorkDuration::new((target + units - 1) / units)?;
        p.tasks
            .iter_mut()
            .find(|t| t.id == id)
            .ok_or("Unknown work task")?
            .duration = duration;
    }
    let policy = p.management.policies.entry(id).or_default();
    policy.target_work_basis = if mode == TaskMode::FixedWork {
        Some(target)
    } else {
        None
    };
    Ok(())
}
pub fn assignment_changed(p: &mut Project, id: TaskId, previous_work: i64) -> Result<()> {
    let policy = p.management.policies.get(&id).cloned().unwrap_or_default();
    if policy.mode == TaskMode::FixedWork {
        set_work(p, id, policy.target_work_basis.unwrap_or(previous_work))?;
    } else if policy.effort_driven && previous_work > 0 {
        set_work(p, id, previous_work)?;
    }
    Ok(())
}
pub fn duration_changed(p: &mut Project, id: TaskId) -> Result<()> {
    let policy = p.management.policies.get(&id).cloned().unwrap_or_default();
    if policy.mode == TaskMode::FixedWork {
        if let Some(target) = policy.target_work_basis {
            set_units_for_work(p, id, target)?;
        }
    }
    Ok(())
}
pub fn validate(p: &Project) -> Result<()> {
    let tasks: BTreeMap<_, _> = p.tasks.iter().map(|t| (t.id, t)).collect();
    let resources: BTreeSet<_> = p.resources.iter().map(|r| r.id).collect();
    for id in p
        .management
        .policies
        .keys()
        .chain(p.management.fixed_costs_cents.keys())
        .chain(p.management.actuals.keys())
    {
        if !tasks.get(id).is_some_and(|t| t.kind == TaskKind::Task) {
            return Err("Management values require a leaf task".into());
        }
    }
    for (&id, policy) in &p.management.policies {
        if let Some(work) = policy.target_work_basis {
            if !(0..=1_000_000_000_000_000_000).contains(&work) {
                return Err("Invalid target work".into());
            }
            if policy.mode == TaskMode::FixedWork && p.assignments.iter().all(|a| a.task != id) {
                return Err("Fixed work requires assignments".into());
            }
        }
    }
    for id in p
        .management
        .hourly_rates_cents
        .keys()
        .chain(p.management.resource_calendars.keys())
    {
        if !resources.contains(id) {
            return Err("Management refers to unknown resource".into());
        }
    }
    for &rate in p.management.hourly_rates_cents.values() {
        if !(0..=1_000_000_000).contains(&rate) {
            return Err("Invalid hourly rate".into());
        }
    }
    for &cost in p.management.fixed_costs_cents.values() {
        if cost < 0 {
            return Err("Fixed cost cannot be negative".into());
        }
    }
    for actual in p.management.actuals.values() {
        if actual.percent_complete > 100
            || actual.work_basis < 0
            || actual.cost_cents < 0
            || matches!((actual.start,actual.finish),(Some(a),Some(b))if a>b)
        {
            return Err("Invalid actual progress/work/cost/dates".into());
        }
    }
    for personal in p.management.resource_calendars.values() {
        for intervals in personal.weekly.iter().chain(personal.exceptions.values()) {
            calendar::intervals(&calendar::intervals_text(intervals))?;
        }
    }
    if p.management.baselines.len() > 20 {
        return Err("Maximum 20 baselines".into());
    }
    let mut names = BTreeSet::new();
    for baseline in &p.management.baselines {
        if baseline.name.is_empty() || baseline.name.len() > 100 || !names.insert(&baseline.name) {
            return Err("Invalid/duplicate baseline name".into());
        }
        baseline.calendar.validate()?;
        let mut ids = BTreeSet::new();
        for t in &baseline.tasks {
            if !ids.insert(t.id)
                || t.es < 0
                || t.ef < t.es
                || t.ef > MAX_HORIZON
                || t.start > t.finish
                || t.cost_cents < 0
                || t.work_basis < 0
            {
                return Err("Invalid baseline task".into());
            }
        }
    }
    if p.management
        .active_baseline
        .as_ref()
        .is_some_and(|name| !names.contains(name))
    {
        return Err("Active baseline does not exist".into());
    }
    if p.management
        .actuals
        .values()
        .map(|a| i128::from(a.cost_cents))
        .sum::<i128>()
        > i128::from(i64::MAX)
    {
        return Err("Actual cost overflow".into());
    }
    for b in &p.management.baselines {
        if b.tasks
            .iter()
            .map(|t| i128::from(t.cost_cents))
            .sum::<i128>()
            > i128::from(i64::MAX)
        {
            return Err("Baseline cost overflow".into());
        }
    }
    if let (Some(date), Some(baseline)) = (
        p.management.status_date,
        p.management
            .active_baseline
            .as_ref()
            .and_then(|name| p.management.baselines.iter().find(|b| &b.name == name)),
    ) {
        Timeline::new(&baseline.calendar)?.start_ceil(Moment(
            date.0 + i64::from(baseline.calendar.utc_offset - p.calendar.utc_offset),
        ))?;
    }
    planned_costs(p)?;
    Ok(())
}
pub fn planned_costs(p: &Project) -> Result<BTreeMap<TaskId, i64>> {
    let mut costs = BTreeMap::new();
    for t in p.tasks.iter().filter(|t| t.kind == TaskKind::Task) {
        let mut cost = i128::from(*p.management.fixed_costs_cents.get(&t.id).unwrap_or(&0));
        for a in p.assignments.iter().filter(|a| a.task == t.id) {
            let rate = *p
                .management
                .hourly_rates_cents
                .get(&a.resource)
                .unwrap_or(&0);
            cost += i128::from(t.duration.minutes())
                * i128::from(a.units.basis_points())
                * i128::from(rate)
                / 600_000;
        }
        costs.insert(
            t.id,
            i64::try_from(cost).map_err(|_| "Planned cost overflow")?,
        );
    }
    let total: i128 = costs.values().map(|&c| i128::from(c)).sum();
    if total > i128::from(i64::MAX) {
        return Err("Total cost overflow".into());
    }
    Ok(costs)
}
pub fn capture(p: &mut Project, name: String) -> Result<()> {
    if p.management.baselines.iter().any(|b| b.name == name) {
        return Err("Baseline snapshots are immutable; choose another name".into());
    }
    let s = schedule(p)?;
    let load = resources::analyze(p, &s)?;
    let costs = planned_costs(p)?;
    let tasks = p
        .tasks
        .iter()
        .zip(&s.tasks)
        .filter(|(t, _)| t.kind == TaskKind::Task)
        .map(|(t, r)| BaselineTask {
            id: t.id,
            start: r.start,
            finish: r.finish,
            es: r.early_start,
            ef: r.early_finish,
            work_basis: *load.task_work_basis.get(&t.id).unwrap_or(&0),
            cost_cents: costs[&t.id],
        })
        .collect();
    p.management.baselines.push(Baseline {
        name: name.clone(),
        calendar: p.calendar.clone(),
        tasks,
    });
    p.management.active_baseline = Some(name);
    Ok(())
}
pub fn clean_task(p: &mut Project, ids: &BTreeSet<TaskId>) {
    p.management.policies.retain(|id, _| !ids.contains(id));
    p.management
        .fixed_costs_cents
        .retain(|id, _| !ids.contains(id));
    p.management.actuals.retain(|id, _| !ids.contains(id));
}
pub fn clean_resource(p: &mut Project, id: ResourceId) {
    p.management.hourly_rates_cents.remove(&id);
    p.management.resource_calendars.remove(&id);
}
pub fn report(
    p: &Project,
    s: &Schedule,
    load: &resources::ResourceReport,
) -> Result<serde_json::Value> {
    let costs = planned_costs(p)?;
    let mut task_rows = Vec::new();
    for (t, r) in p
        .tasks
        .iter()
        .zip(&s.tasks)
        .filter(|(t, _)| t.kind == TaskKind::Task)
    {
        let baseline = p
            .management
            .active_baseline
            .as_ref()
            .and_then(|name| p.management.baselines.iter().find(|b| &b.name == name))
            .and_then(|b| b.tasks.iter().find(|b| b.id == t.id));
        task_rows.push(serde_json::json!({"id":t.id,"policy":p.management.policies.get(&t.id).cloned().unwrap_or_default(),"planned_cost_cents":costs[&t.id],"actual":p.management.actuals.get(&t.id).cloned().unwrap_or_default(),"work_basis":load.task_work_basis.get(&t.id).copied().unwrap_or(0),"baseline_start":baseline.map(|b|b.start),"baseline_finish":baseline.map(|b|b.finish),"current_start":r.start,"current_finish":r.finish}));
    }
    let mut evm = serde_json::Value::Null;
    if let Some(baseline) = p
        .management
        .active_baseline
        .as_ref()
        .and_then(|name| p.management.baselines.iter().find(|b| &b.name == name))
    {
        let mut tl = Timeline::new(&baseline.calendar)?;
        let status = p
            .management
            .status_date
            .map(|d| {
                tl.start_ceil(Moment(
                    d.0 + i64::from(baseline.calendar.utc_offset - p.calendar.utc_offset),
                ))
            })
            .transpose()?;
        let (mut bac, mut pv, mut ev, mut ac) = (0i128, 0i128, 0i128, 0i128);
        for t in &baseline.tasks {
            let actual = p.management.actuals.get(&t.id).cloned().unwrap_or_default();
            bac += i128::from(t.cost_cents);
            ev += i128::from(t.cost_cents) * i128::from(actual.percent_complete) / 100;
            ac += i128::from(actual.cost_cents);
            if let Some(status) = status {
                pv += if t.ef == t.es {
                    if status >= t.es {
                        i128::from(t.cost_cents)
                    } else {
                        0
                    }
                } else {
                    i128::from(t.cost_cents) * i128::from((status - t.es).clamp(0, t.ef - t.es))
                        / i128::from(t.ef - t.es)
                };
            }
        }
        evm = serde_json::json!({"baseline":baseline.name,"bac_cents":bac,"pv_cents":if status.is_some(){Some(pv)}else{None},"ev_cents":ev,"ac_cents":ac,"cpi":if ac>0{Some(ev as f64/ac as f64)}else{None},"spi":if pv>0{Some(ev as f64/pv as f64)}else{None}});
    }
    Ok(
        serde_json::json!({"settings":p.management,"tasks":task_rows,"planned_cost_cents":costs.values().map(|&c|i128::from(c)).sum::<i128>(),"evm":evm}),
    )
}
