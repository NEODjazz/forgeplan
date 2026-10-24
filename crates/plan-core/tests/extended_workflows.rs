use plan_core::management::{Actual, PersonalCalendar, TaskMode, TaskPolicy};
use plan_core::*;
fn p() -> Project {
    parse(include_str!("../../../examples/infrastructure.fplan")).unwrap()
}
fn tid(n: u32) -> TaskId {
    TaskId::new(n).unwrap()
}
fn rid(n: u32) -> ResourceId {
    ResourceId::new(n).unwrap()
}
fn apply(e: &mut Editor, c: Vec<Command>) {
    e.apply(e.revision(), c).unwrap()
}
#[test]
fn json_is_canonical_and_validates_private_newtypes() {
    let p = p();
    let text = serialize(&p).unwrap();
    assert_eq!(serialize(&parse(&text).unwrap()).unwrap(), text);
    let mut v: serde_json::Value = serde_json::from_str(&text).unwrap();
    v["schema_version"] = 99.into();
    assert!(parse(&v.to_string()).is_err());
    v["schema_version"] = 1.into();
    v["project"]["tasks"][0]["id"] = 0.into();
    assert!(parse(&v.to_string()).is_err());
    let mut v: serde_json::Value = serde_json::from_str(&text).unwrap();
    v["project"]["not_a_field"] = 1.into();
    assert!(parse(&v.to_string()).is_err());
}
#[test]
fn fixed_work_preserves_target_and_updates_duration_when_units_change() {
    let mut e = Editor::new(p()).unwrap();
    apply(
        &mut e,
        vec![Command::Policy {
            id: tid(2),
            policy: TaskPolicy {
                mode: TaskMode::FixedWork,
                effort_driven: true,
                target_work_basis: None,
            },
        }],
    );
    let target = management::work(e.project(), tid(2));
    apply(
        &mut e,
        vec![Command::SetAssignments {
            task: tid(2),
            assignments: vec![Assignment {
                task: tid(2),
                resource: rid(1),
                units: Units::new(5000).unwrap(),
            }],
        }],
    );
    assert_eq!(management::work(e.project(), tid(2)), target);
    assert_eq!(
        e.project()
            .tasks
            .iter()
            .find(|t| t.id == tid(2))
            .unwrap()
            .duration
            .minutes(),
        4800
    );
    e.undo(e.revision()).unwrap();
    assert_eq!(
        e.project()
            .tasks
            .iter()
            .find(|t| t.id == tid(2))
            .unwrap()
            .duration
            .minutes(),
        2400
    );
}
#[test]
fn fixed_duration_work_scales_units_and_fixed_units_work_scales_duration() {
    let mut e = Editor::new(p()).unwrap();
    apply(
        &mut e,
        vec![Command::Work {
            id: tid(2),
            target_basis: 1200 * 10000,
        }],
    );
    assert_eq!(
        e.project()
            .assignments
            .iter()
            .find(|a| a.task == tid(2))
            .unwrap()
            .units
            .basis_points(),
        5000
    );
    assert_eq!(
        e.project()
            .tasks
            .iter()
            .find(|t| t.id == tid(2))
            .unwrap()
            .duration
            .minutes(),
        2400
    );
    apply(
        &mut e,
        vec![
            Command::Policy {
                id: tid(2),
                policy: TaskPolicy {
                    mode: TaskMode::FixedUnits,
                    effort_driven: false,
                    target_work_basis: None,
                },
            },
            Command::Work {
                id: tid(2),
                target_basis: 600 * 10000,
            },
        ],
    );
    assert_eq!(
        e.project()
            .tasks
            .iter()
            .find(|t| t.id == tid(2))
            .unwrap()
            .duration
            .minutes(),
        1200
    );
}
#[test]
fn personal_calendar_changes_capacity_without_silently_moving_dates() {
    let mut p = p();
    let before = schedule(&p).unwrap();
    let mut weekly = p.calendar.weekly.clone();
    weekly[0] = calendar::intervals("09:00-13:00").unwrap();
    p.management.resource_calendars.insert(
        rid(1),
        PersonalCalendar {
            weekly,
            exceptions: Default::default(),
        },
    );
    let after = schedule(&p).unwrap();
    assert_eq!(before.finish, after.finish);
    let a = resources::analyze(&p, &after).unwrap();
    assert!(a.resources[0].capacity_basis < 108 * 60 * 10000);
}
#[test]
fn rates_baseline_actual_and_evm_use_immutable_snapshot_costs() {
    let mut e = Editor::new(p()).unwrap();
    apply(
        &mut e,
        vec![
            Command::Rate {
                id: rid(1),
                cents_per_hour: 6000,
            },
            Command::Baseline {
                name: "Approved".into(),
            },
            Command::Actual {
                id: tid(1),
                actual: Actual {
                    percent_complete: 50,
                    work_basis: 12 * 60 * 10000,
                    cost_cents: 60000,
                    start: None,
                    finish: None,
                },
            },
            Command::StatusDate(Some(Moment::parse("2026-10-27T09:00").unwrap())),
        ],
    );
    let baseline = e.project().management.baselines[0].clone();
    let report = management::report(
        e.project(),
        &schedule(e.project()).unwrap(),
        &resources::analyze(e.project(), &schedule(e.project()).unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(report["evm"]["ev_cents"], 72000);
    assert_eq!(report["evm"]["ac_cents"], 60000);
    apply(
        &mut e,
        vec![Command::Rate {
            id: rid(1),
            cents_per_hour: 12000,
        }],
    );
    assert_eq!(e.project().management.baselines[0], baseline);
    let before = e.revision();
    assert!(e
        .apply(
            before,
            vec![Command::Baseline {
                name: "Approved".into()
            }]
        )
        .is_err());
    assert_eq!(e.revision(), before);
}
#[test]
fn leveling_preview_is_non_mutating_and_apply_is_undoable() {
    let mut project = p();
    project.resources[0].availability.clear();
    let original = serialize(&project).unwrap();
    let preview = leveling::preview(&project, false).unwrap();
    assert_eq!(serialize(&project).unwrap(), original);
    assert_eq!(preview.unresolved, 0);
    assert!(!preview.shifts.is_empty());
    let mut e = Editor::new(project).unwrap();
    apply(&mut e, vec![Command::Replace(Box::new(preview.project))]);
    assert_eq!(
        resources::analyze(e.project(), &schedule(e.project()).unwrap())
            .unwrap()
            .resources[0]
            .overloaded_minutes,
        0
    );
    e.undo(e.revision()).unwrap();
    assert_eq!(serialize(e.project()).unwrap(), original);
}
#[test]
fn xml_subset_roundtrip_reports_loss_and_rejects_dtd() {
    let p = p();
    let out = exchange::export_xml(&p).unwrap();
    assert!(!out.warnings.is_empty());
    let imported = exchange::import_xml(&out.xml).unwrap();
    assert_eq!(imported.project.tasks.len(), p.tasks.len());
    assert_eq!(imported.project.links, p.links);
    assert_eq!(imported.project.assignments, p.assignments);
    assert!(!imported.warnings.is_empty());
    assert!(exchange::import_xml(
        "<!DOCTYPE Project [<!ENTITY x SYSTEM 'file:///etc/passwd'>]><Project/>"
    )
    .is_err());
}
#[test]
fn csv_quotes_and_formula_protection_are_explicit() {
    let mut p = p();
    p.tasks[1].title = "Name, with \"quotes\"".into();
    let csv = exchange::export_csv(&p).unwrap();
    let imported = exchange::import_csv(&csv).unwrap();
    assert!(imported
        .project
        .tasks
        .iter()
        .any(|t| t.title == "Name, with \"quotes\""));
    p.tasks[1].title = "=1+1".into();
    assert!(exchange::export_csv(&p).unwrap().contains("'=1+1"));
}

#[test]
fn typed_replace_roundtrips_nonempty_id_keyed_maps() {
    let mut p = p();
    p.management.hourly_rates_cents.insert(rid(1), 6000);
    let command = Command::Replace(Box::new(p));
    let text = serde_json::to_string(&command).unwrap();
    let decoded: Command = serde_json::from_str(&text).unwrap();
    assert_eq!(serde_json::to_string(&decoded).unwrap(), text);
}

#[test]
fn status_date_is_translated_into_the_baseline_fixed_offset() {
    let mut p = p();
    p.management.hourly_rates_cents.insert(rid(1), 6000);
    management::capture(&mut p, "B".into()).unwrap();
    p.management.status_date = Some(Moment::parse("2026-10-26T13:00").unwrap());
    let s = schedule(&p).unwrap();
    let before = management::report(&p, &s, &resources::analyze(&p, &s).unwrap()).unwrap()["evm"]
        ["pv_cents"]
        .clone();
    p.calendar.utc_offset = 0;
    p.management.status_date = Some(Moment::parse("2026-10-26T10:00").unwrap());
    let s = schedule(&p).unwrap();
    let after = management::report(&p, &s, &resources::analyze(&p, &s).unwrap()).unwrap();
    assert_eq!(after["evm"]["pv_cents"], before);
}
#[test]
fn replacing_an_existing_baseline_in_place_is_rejected() {
    let mut e = Editor::new(p()).unwrap();
    apply(&mut e, vec![Command::Baseline { name: "B".into() }]);
    let mut edited = e.project().clone();
    edited.management.baselines[0].tasks[0].cost_cents += 100;
    assert!(e
        .apply(e.revision(), vec![Command::Replace(Box::new(edited))])
        .is_err());
}

#[test]
fn history_compaction_preserves_current_model_and_revision() {
    let mut e = Editor::new(p()).unwrap();
    apply(
        &mut e,
        vec![Command::Rate {
            id: rid(1),
            cents_per_hour: 6000,
        }],
    );
    let current = serialize(e.project()).unwrap();
    let revision = e.revision();
    let budget = serde_json::to_vec(&Editor::new(e.project().clone()).unwrap())
        .unwrap()
        .len()
        + 200;
    e.compact_history(budget).unwrap();
    assert_eq!(e.revision(), revision);
    assert_eq!(serialize(e.project()).unwrap(), current);
    assert!(!e.can_undo());
}
