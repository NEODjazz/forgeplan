//! Explicitly limited interchange with diagnostics; native JSON is lossless.
use crate::*;
use serde::{Deserialize, Serialize};
#[derive(Serialize)]
pub struct Imported {
    pub project: Project,
    pub warnings: Vec<String>,
}
#[derive(Serialize)]
pub struct Exported {
    pub xml: String,
    pub warnings: Vec<String>,
}
#[derive(Default, Deserialize)]
#[serde(default)]
struct XmlProject {
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "StartDate")]
    start: String,
    #[serde(rename = "Tasks")]
    tasks: XmlTasks,
    #[serde(rename = "Resources")]
    resources: XmlResources,
    #[serde(rename = "Assignments")]
    assignments: XmlAssignments,
}
#[derive(Default, Deserialize)]
struct XmlTasks {
    #[serde(rename = "Task", default)]
    tasks: Vec<XmlTask>,
}
#[derive(Default, Deserialize)]
#[serde(default)]
struct XmlTask {
    #[serde(rename = "UID")]
    id: u32,
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "OutlineLevel")]
    level: usize,
    #[serde(rename = "Summary")]
    summary: u8,
    #[serde(rename = "Duration")]
    duration: String,
    #[serde(rename = "PredecessorLink")]
    links: Vec<XmlLink>,
}
#[derive(Default, Deserialize)]
#[serde(default)]
struct XmlLink {
    #[serde(rename = "PredecessorUID")]
    from: u32,
    #[serde(rename = "Type")]
    kind: u8,
    #[serde(rename = "LinkLag")]
    lag: i64,
}
#[derive(Default, Deserialize)]
struct XmlResources {
    #[serde(rename = "Resource", default)]
    resources: Vec<XmlResource>,
}
#[derive(Default, Deserialize)]
#[serde(default)]
struct XmlResource {
    #[serde(rename = "UID")]
    id: u32,
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "MaxUnits")]
    units: String,
    #[serde(rename = "Type")]
    kind: u8,
}
#[derive(Default, Deserialize)]
struct XmlAssignments {
    #[serde(rename = "Assignment", default)]
    assignments: Vec<XmlAssignment>,
}
#[derive(Default, Deserialize)]
#[serde(default)]
struct XmlAssignment {
    #[serde(rename = "TaskUID")]
    task: u32,
    #[serde(rename = "ResourceUID")]
    resource: u32,
    #[serde(rename = "Units")]
    units: String,
}
fn decimal_units(s: &str) -> Result<Units> {
    let parts: Vec<_> = s.split('.').collect();
    if parts.len() > 2 || parts[0].is_empty() || !parts[0].bytes().all(|c| c.is_ascii_digit()) {
        return Err("Invalid XML units".into());
    }
    let whole: u32 = parts[0].parse().map_err(|_| "Units overflow")?;
    let fraction = parts.get(1).copied().unwrap_or("");
    if fraction.len() > 4 || !fraction.bytes().all(|c| c.is_ascii_digit()) {
        return Err("XML units require at most four decimal places".into());
    }
    let fraction: u32 = format!("{fraction:0<4}")
        .parse()
        .map_err(|_| "Invalid units")?;
    Units::new(
        whole
            .checked_mul(10000)
            .and_then(|n| n.checked_add(fraction))
            .ok_or("Units overflow")?,
    )
}
fn duration(s: &str) -> Result<WorkDuration> {
    if !s.starts_with("PT") {
        return Err("Only whole-minute PTnHnMnS XML durations are supported".into());
    }
    let mut digits = String::new();
    let mut minutes = 0i64;
    for c in s[2..].chars() {
        if c.is_ascii_digit() {
            digits.push(c);
        } else {
            let n: i64 = digits.parse().map_err(|_| "Invalid XML duration")?;
            digits.clear();
            let add = match c {
                'H' => n.checked_mul(60).ok_or("Duration overflow")?,
                'M' => n,
                'S' if n == 0 => 0,
                _ => return Err("Subminute/unsupported XML duration".into()),
            };
            minutes = minutes.checked_add(add).ok_or("Duration overflow")?;
        }
    }
    if !digits.is_empty() {
        return Err("Invalid duration suffix".into());
    }
    WorkDuration::new(minutes)
}
pub fn import_xml(text: &str) -> Result<Imported> {
    if text.len() > 8_000_000 {
        return Err("XML exceeds 8 MB".into());
    }
    let mut reader = quick_xml::Reader::from_str(text);
    let mut unknown = BTreeSet::new();
    let supported = [
        "Project",
        "Name",
        "StartDate",
        "Tasks",
        "Task",
        "UID",
        "ID",
        "OutlineLevel",
        "OutlineNumber",
        "WBS",
        "Summary",
        "Duration",
        "PredecessorLink",
        "PredecessorUID",
        "Type",
        "LinkLag",
        "Resources",
        "Resource",
        "MaxUnits",
        "Assignments",
        "Assignment",
        "TaskUID",
        "ResourceUID",
        "Units",
    ];
    loop {
        match reader.read_event().map_err(|e| e.to_string())? {
            quick_xml::events::Event::DocType(_) => {
                return Err("XML DTD/entity declarations are not allowed".into())
            }
            quick_xml::events::Event::Start(e) | quick_xml::events::Event::Empty(e) => {
                let tag = String::from_utf8_lossy(e.local_name().as_ref()).into_owned();
                if !supported.contains(&tag.as_str()) {
                    unknown.insert(tag);
                }
            }
            quick_xml::events::Event::Eof => break,
            _ => {}
        }
    }
    let xml: XmlProject = quick_xml::de::from_str(text).map_err(|e| e.to_string())?;
    if xml.start.len() < 16 || !xml.start.is_ascii() {
        return Err("Project XML requires StartDate".into());
    }
    let mut calendar = Calendar::weekdays(Date::parse(&xml.start[..10])?);
    calendar.start_minute = calendar::parse_time(&xml.start[11..16], false)?;
    let mut p = Project {
        title: xml.name,
        calendar,
        tasks: vec![],
        links: vec![],
        resources: vec![],
        assignments: vec![],
        management: Management::default(),
    };
    let mut stack = Vec::<TaskId>::new();
    let mut order = BTreeMap::<Option<TaskId>, u32>::new();
    for task in xml.tasks.tasks.into_iter().filter(|t| t.id != 0) {
        let level = task.level.max(1);
        if level > stack.len() + 1 {
            return Err("XML outline has a missing parent".into());
        }
        stack.truncate(level - 1);
        let parent = stack.last().copied();
        let id = TaskId::new(task.id)?;
        let pos = order.entry(parent).or_insert(0);
        let kind = if task.summary == 1 {
            TaskKind::Summary
        } else {
            TaskKind::Task
        };
        p.tasks.push(Task {
            id,
            parent,
            order: *pos,
            kind,
            title: task.name,
            duration: if kind == TaskKind::Summary {
                WorkDuration::new(0)?
            } else {
                duration(&task.duration)?
            },
            constraint: None,
            deadline: None,
        });
        *pos += 1;
        stack.push(id);
        for link in task.links {
            if link.lag % 10 != 0 {
                return Err("XML lag has subminute precision".into());
            }
            let relation = match link.kind {
                0 => Relation::FF,
                1 => Relation::FS,
                2 => Relation::SF,
                3 => Relation::SS,
                _ => return Err("Unknown XML predecessor type".into()),
            };
            p.links.push(Dependency {
                from: TaskId::new(link.from)?,
                to: id,
                relation,
                lag: Lag::new(link.lag / 10)?,
            });
        }
    }
    for resource in xml.resources.resources.into_iter().filter(|r| r.id != 0) {
        if resource.kind != 1 {
            return Err("Only XML work resources (Type=1) are supported".into());
        }
        p.resources.push(Resource {
            id: ResourceId::new(resource.id)?,
            name: resource.name,
            kind: ResourceKind::Person,
            capacity: decimal_units(if resource.units.is_empty() {
                "1"
            } else {
                &resource.units
            })?,
            availability: vec![],
        });
    }
    for a in xml.assignments.assignments {
        p.assignments.push(Assignment {
            task: TaskId::new(a.task)?,
            resource: ResourceId::new(a.resource)?,
            units: decimal_units(&a.units)?,
        });
    }
    schedule(&p)?;
    let mut warnings=vec!["XML subset import uses a Mon-Fri 09:00-17:00 calendar and fixed UTC+00:00; review dates before applying".into(),"Only hierarchy, durations, predecessor links and work resource assignments are imported; native JSON preserves the full model".into()];
    warnings.extend(
        unknown
            .into_iter()
            .map(|tag| format!("Unsupported XML element: {tag}")),
    );
    Ok(Imported {
        project: p,
        warnings,
    })
}
pub fn export_xml(p: &Project) -> Result<Exported> {
    schedule(p)?;
    let mut xml=format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?><Project xmlns=\"http://schemas.microsoft.com/project\"><Name>{}</Name><StartDate>{}T{}:00</StartDate><Tasks>",quick_xml::escape::escape(&p.title),p.calendar.start.iso(),calendar::time_text(p.calendar.start_minute));
    for row in outline(p)? {
        let t = &p.tasks[row.index];
        xml.push_str(&format!("<Task><UID>{}</UID><ID>{}</ID><Name>{}</Name><OutlineLevel>{}</OutlineLevel><Summary>{}</Summary><Duration>PT{}H{}M0S</Duration>",t.id,t.id,quick_xml::escape::escape(&t.title),row.depth+1,usize::from(t.kind==TaskKind::Summary),t.duration.minutes()/60,t.duration.minutes()%60));
        for l in p.links.iter().filter(|l| l.to == t.id) {
            let kind = match l.relation {
                Relation::FF => 0,
                Relation::FS => 1,
                Relation::SF => 2,
                Relation::SS => 3,
            };
            xml.push_str(&format!("<PredecessorLink><PredecessorUID>{}</PredecessorUID><Type>{}</Type><LinkLag>{}</LinkLag></PredecessorLink>",l.from,kind,l.lag.minutes()*10));
        }
        xml.push_str("</Task>");
    }
    xml.push_str("</Tasks><Resources>");
    for r in &p.resources {
        xml.push_str(&format!("<Resource><UID>{}</UID><Name>{}</Name><Type>1</Type><MaxUnits>{}</MaxUnits></Resource>",r.id,quick_xml::escape::escape(&r.name),resources::work_decimal(i64::from(r.capacity.basis_points()))));
    }
    xml.push_str("</Resources><Assignments>");
    for a in &p.assignments {
        xml.push_str(&format!("<Assignment><TaskUID>{}</TaskUID><ResourceUID>{}</ResourceUID><Units>{}</Units></Assignment>",a.task,a.resource,resources::work_decimal(i64::from(a.units.basis_points()))));
    }
    xml.push_str("</Assignments></Project>");
    Ok(Exported{xml,warnings:vec!["XML export is a subset: custom calendars/timezone, constraints, baselines, actuals, costs and resource availability are not exported".into(),"Generic resources export as work resources. Microsoft Project application acceptance has not been verified".into()]})
}
fn cell(s: &str) -> String {
    let s = if s.starts_with(['=', '+', '-', '@']) {
        format!("'{s}")
    } else {
        s.into()
    };
    format!("\"{}\"", s.replace('"', "\"\""))
}
pub fn export_csv(p: &Project) -> Result<String> {
    let s = schedule(p)?;
    let mut out = String::from("id,parent,order,kind,title,duration_minutes,start,finish\r\n");
    for row in outline(p)? {
        let t = &p.tasks[row.index];
        let r = &s.tasks[row.index];
        out.push_str(
            &[
                t.id.to_string(),
                t.parent.map(|id| id.to_string()).unwrap_or_default(),
                t.order.to_string(),
                t.kind.label().into(),
                cell(&t.title),
                t.duration.to_string(),
                r.start.iso(p.calendar.utc_offset),
                r.finish.iso(p.calendar.utc_offset),
            ]
            .join(","),
        );
        out.push_str("\r\n");
    }
    Ok(out)
}
fn csv_rows(text: &str) -> Result<Vec<Vec<String>>> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut cell = String::new();
    let mut chars = text.chars().peekable();
    let mut quoted = false;
    while let Some(c) = chars.next() {
        if quoted {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    cell.push('"');
                } else {
                    quoted = false;
                }
            } else {
                cell.push(c);
            }
        } else {
            match c {
                '"' if cell.is_empty() => quoted = true,
                ',' => row.push(std::mem::take(&mut cell)),
                '\n' => {
                    row.push(std::mem::take(&mut cell));
                    rows.push(std::mem::take(&mut row));
                }
                '\r' if chars.peek() == Some(&'\n') => {}
                _ => cell.push(c),
            }
        }
    }
    if quoted {
        return Err("Unclosed CSV quote".into());
    }
    if !cell.is_empty() || !row.is_empty() {
        row.push(cell);
        rows.push(row);
    }
    Ok(rows)
}
pub fn import_csv(text: &str) -> Result<Imported> {
    if text.len() > 8_000_000 {
        return Err("CSV exceeds 8 MB".into());
    }
    let mut rows = csv_rows(text)?;
    if rows.is_empty()
        || rows.remove(0)
            != [
                "id",
                "parent",
                "order",
                "kind",
                "title",
                "duration_minutes",
                "start",
                "finish",
            ]
    {
        return Err("Expected ForgePlan CSV column header".into());
    }
    let mut p = Project {
        title: "CSV import".into(),
        calendar: Calendar::weekdays(Date::parse("2026-01-01")?),
        tasks: vec![],
        links: vec![],
        resources: vec![],
        assignments: vec![],
        management: Management::default(),
    };
    for row in rows {
        if row.len() != 8 {
            return Err("CSV row must have eight columns".into());
        }
        let kind = TaskKind::parse(&row[3])?;
        p.tasks.push(Task {
            id: row[0].parse()?,
            parent: if row[1].is_empty() {
                None
            } else {
                Some(row[1].parse()?)
            },
            order: row[2].parse().map_err(|_| "Invalid order")?,
            kind,
            title: row[4].clone(),
            duration: row[5].parse()?,
            constraint: None,
            deadline: None,
        });
    }
    schedule(&p)?;
    Ok(Imported{project:p,warnings:vec!["CSV imports task hierarchy and duration only; calendar defaults to 2026-01-01, dates are recalculated, links/resources/costs are not imported".into()]})
}
