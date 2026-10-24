//! Local prototype wire format, not the future enterprise API.
use plan_core::*;
use std::collections::{BTreeMap, BTreeSet};

pub enum Action {
    Batch(Vec<Command>),
    Undo,
    Redo,
}
pub fn parse_edit(text: &str) -> Result<(Revision, Action)> {
    if text.trim_start().starts_with('{') {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Envelope {
            base_revision: u64,
            commands: Vec<Command>,
        }
        if text.len() > 8_000_000 {
            return Err("Command body exceeds 8 MB".into());
        }
        let envelope: Envelope =
            serde_json::from_str(text).map_err(|e| format!("Invalid command JSON: {e}"))?;
        return Ok((
            Revision::new(envelope.base_revision),
            Action::Batch(envelope.commands),
        ));
    }
    if text.len() > 8_000_000 {
        return Err("Command body exceeds 8 MB".into());
    }
    let (header, body) = text.split_once('\n').ok_or("Missing command body")?;
    let revision = header
        .trim_end_matches('\r')
        .strip_prefix("revision\t")
        .ok_or("Expected revision header")?
        .parse::<u64>()
        .map_err(|_| "Invalid revision")?;
    let revision = Revision::new(revision);
    if body.trim_end() == "undo" {
        return Ok((revision, Action::Undo));
    }
    if body.trim_end() == "redo" {
        return Ok((revision, Action::Redo));
    }
    if let Some(document) = body
        .strip_prefix("replace\n")
        .or_else(|| body.strip_prefix("replace\r\n"))
    {
        return Ok((
            revision,
            Action::Batch(vec![Command::Replace(Box::new(parse(document)?))]),
        ));
    }
    let mut commands = Vec::new();
    for line in body.lines() {
        if line.is_empty() {
            continue;
        }
        let r: Vec<_> = line.split('\t').collect();
        commands.push(match r.as_slice() {
            ["add", id, parent, before, kind, duration, title] => Command::Add {
                task: Task {
                    id: id.parse()?,
                    parent: parse_parent(parent)?,
                    order: 0,
                    kind: TaskKind::parse(kind)?,
                    constraint: None,
                    deadline: None,
                    duration: duration.parse()?,
                    title: (*title).into(),
                },
                before: parse_parent(before)?,
            },
            ["update", id, duration, title] => Command::Update {
                id: id.parse()?,
                duration: duration.parse()?,
                title: (*title).into(),
            },
            ["move", id, parent, before] => Command::Move {
                id: id.parse()?,
                parent: parse_parent(parent)?,
                before: parse_parent(before)?,
            },
            ["delete", id] => Command::DeleteSubtree { id: id.parse()? },
            ["predecessors", id, predecessors] => {
                let id: TaskId = id.parse()?;
                let mut links = Vec::new();
                for link in predecessors
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                {
                    let cols: Vec<_> = link.split(':').collect();
                    let [from, relation, lag] = cols.as_slice() else {
                        return Err("Expected predecessor ID:FS:lag".into());
                    };
                    links.push(Dependency {
                        from: from.parse()?,
                        to: id,
                        relation: Relation::parse(relation)?,
                        lag: lag.parse()?,
                    });
                }
                Command::SetPredecessors { id, links }
            }
            ["project", title, start, holidays] => {
                let holidays = holidays
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(Date::parse)
                    .collect::<Result<BTreeSet<_>>>()?;
                Command::ProjectSettings {
                    title: (*title).into(),
                    start: Date::parse(start)?,
                    holidays,
                }
            }
            ["dates", id, kind, at, deadline] => {
                let constraint = if *kind == "-" && *at == "-" {
                    None
                } else {
                    Some(Constraint {
                        kind: ConstraintType::parse(kind)?,
                        at: Moment::parse(at)?,
                    })
                };
                let deadline = if *deadline == "-" {
                    None
                } else {
                    Some(Moment::parse(deadline)?)
                };
                Command::Dates {
                    id: id.parse()?,
                    constraint,
                    deadline,
                }
            }
            ["calendar", start, offset, week, exception_text] => {
                let values = week
                    .split(';')
                    .map(calendar::intervals)
                    .collect::<Result<Vec<_>>>()?;
                let weekly: [Vec<WorkInterval>; 7] = values
                    .try_into()
                    .map_err(|_| "Expected 7 weekday schedules")?;
                let mut exceptions = BTreeMap::new();
                if *exception_text != "-" {
                    for item in exception_text.split(';') {
                        let (date, hours) = item
                            .split_once('=')
                            .ok_or("Expected date=intervals exception")?;
                        if exceptions
                            .insert(Date::parse(date)?, calendar::intervals(hours)?)
                            .is_some()
                        {
                            return Err("Duplicate exception".into());
                        }
                    }
                }
                Command::CalendarSettings {
                    start_minute: calendar::parse_time(start, false)?,
                    utc_offset: calendar::parse_offset(offset)?,
                    weekly,
                    exceptions,
                }
            }
            [operation, id, kind, capacity, name, periods]
                if *operation == "resource-add" || *operation == "resource-update" =>
            {
                let mut availability = Vec::new();
                if *periods != "-" {
                    for period in periods.split(';') {
                        let (dates, units) = period
                            .split_once(':')
                            .ok_or("Expected from..through:basis_points")?;
                        let (from, through) = dates
                            .split_once("..")
                            .ok_or("Expected availability date range")?;
                        availability.push(Availability {
                            from: Date::parse(from)?,
                            through: Date::parse(through)?,
                            capacity: units.parse()?,
                        });
                    }
                }
                let resource = Resource {
                    id: id.parse()?,
                    kind: ResourceKind::parse(kind)?,
                    capacity: capacity.parse()?,
                    name: (*name).into(),
                    availability,
                };
                if *operation == "resource-add" {
                    Command::AddResource(resource)
                } else {
                    Command::UpdateResource(resource)
                }
            }
            ["resource-delete", id] => Command::DeleteResource(id.parse()?),
            ["assignments", task, entries] => {
                let task: TaskId = task.parse()?;
                let mut assignments = Vec::new();
                for entry in entries.split(',').map(str::trim).filter(|v| !v.is_empty()) {
                    let (resource, units) = entry
                        .split_once(':')
                        .ok_or("Expected resource_id:basis_points")?;
                    assignments.push(Assignment {
                        task,
                        resource: resource.parse()?,
                        units: units.parse()?,
                    });
                }
                Command::SetAssignments { task, assignments }
            }
            ["json", command] => serde_json::from_str::<Command>(command)
                .map_err(|e| format!("Invalid typed command: {e}"))?,
            _ => return Err("Invalid command row".into()),
        });
        if commands.len() > 10000 {
            return Err("Too many commands".into());
        }
    }
    if commands.is_empty() {
        return Err("No commands".into());
    }
    Ok((revision, Action::Batch(commands)))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strict_protocol() {
        assert!(parse_edit("revision\t0\nundo\n").is_ok());
        assert!(parse_edit("revision\t0\nadd\t1\t-\t-\ttask\t2\tA\n").is_ok());
        assert!(parse_edit("revision\t0\npredecessors\t1\t2:FS:-2\n").is_ok());
        for s in [
            "revision\t-1\nundo",
            "revision\t0\n",
            "revision\t0\nundo\nredo",
            "revision\t0\nadd\t0\t-\t-\ttask\t1\tA",
            "revision\t0\npredecessors\t1\t2:BAD:0",
        ] {
            assert!(parse_edit(s).is_err());
        }
    }
}
