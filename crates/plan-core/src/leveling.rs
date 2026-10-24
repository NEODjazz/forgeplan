use crate::*;
use serde::Serialize;
#[derive(Serialize)]
pub struct Shift {
    pub id: TaskId,
    pub before: Moment,
    pub after: Moment,
}
#[derive(Serialize)]
pub struct Preview {
    pub project: Project,
    pub shifts: Vec<Shift>,
    pub unresolved: usize,
    pub warnings: Vec<String>,
}
pub fn preview(p: &Project, only_within_float: bool) -> Result<Preview> {
    let original = schedule(p)?;
    let mut draft = p.clone();
    let mut warnings = Vec::new();
    for _ in 0..128 {
        let s = schedule(&draft)?;
        let load = resources::analyze(&draft, &s)?;
        let Some(overload) = load
            .resources
            .iter()
            .flat_map(|r| r.overloads.iter())
            .min_by_key(|o| o.start)
        else {
            break;
        };
        let mut moved = false;
        let mut ids = overload.task_ids.clone();
        ids.sort_by_key(|id| std::cmp::Reverse(id.get()));
        for id in ids {
            let Some(task) = draft.tasks.iter().find(|t| t.id == id) else {
                continue;
            };
            if task
                .constraint
                .is_some_and(|c| c.kind != ConstraintType::SNET)
            {
                continue;
            }
            let mut candidate = draft.clone();
            let index = candidate.tasks.iter().position(|t| t.id == id).unwrap();
            candidate.tasks[index].constraint = Some(Constraint {
                kind: ConstraintType::SNET,
                at: overload.finish,
            });
            if let Ok(after) = schedule(&candidate) {
                let before_task = s.tasks.iter().find(|t| t.id == id).unwrap();
                let after_task = after.tasks.iter().find(|t| t.id == id).unwrap();
                if after_task.start <= before_task.start
                    || only_within_float && after.finish > original.finish
                {
                    continue;
                }
                draft = candidate;
                moved = true;
                break;
            }
        }
        if !moved {
            warnings.push("Cannot resolve the conflict without violating a constraint or the requested project finish".into());
            break;
        }
    }
    let after = schedule(&draft)?;
    let load = resources::analyze(&draft, &after)?;
    let unresolved = load.resources.iter().map(|r| r.overload_count).sum();
    if unresolved > 0 {
        warnings.push("Preview has unresolved overloads and cannot be applied".into());
    }
    let shifts = original
        .tasks
        .iter()
        .filter_map(|before| {
            let t = after.tasks.iter().find(|t| t.id == before.id)?;
            if !before.summary && before.start != t.start {
                Some(Shift {
                    id: before.id,
                    before: before.start,
                    after: t.start,
                })
            } else {
                None
            }
        })
        .collect();
    Ok(Preview {
        project: draft,
        shifts,
        unresolved,
        warnings,
    })
}
