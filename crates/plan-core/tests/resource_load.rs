use plan_core::resources::{analyze, work_decimal, DAILY_LIMIT, OVERLOAD_LIMIT};
use plan_core::*;
fn tid(n: u32) -> TaskId {
    TaskId::new(n).unwrap()
}
fn rid(n: u32) -> ResourceId {
    ResourceId::new(n).unwrap()
}
fn u(bp: u32) -> Units {
    Units::new(bp).unwrap()
}
fn date(s: &str) -> Date {
    Date::parse(s).unwrap()
}
fn resource(id: u32, capacity: u32) -> Resource {
    Resource {
        id: rid(id),
        name: format!("R{id}"),
        kind: ResourceKind::Person,
        capacity: u(capacity),
        availability: vec![],
    }
}
fn base(durations: &[i64]) -> Project {
    let mut p = parse(include_str!("../../../examples/legacy-v3.fplan")).unwrap();
    p.tasks = durations
        .iter()
        .enumerate()
        .map(|(i, &n)| Task {
            id: tid(i as u32 + 1),
            title: format!("T{i}"),
            duration: WorkDuration::new(n).unwrap(),
            parent: None,
            order: i as u32,
            kind: TaskKind::Task,
            constraint: None,
            deadline: None,
        })
        .collect();
    p.links.clear();
    p.resources = vec![resource(1, 10000)];
    p.assignments = vec![];
    p
}
fn assign(p: &mut Project, task: u32, resource: u32, units: u32) {
    p.assignments.push(Assignment {
        task: tid(task),
        resource: rid(resource),
        units: u(units),
    });
}
fn report(p: &Project) -> resources::ResourceReport {
    analyze(p, &schedule(p).unwrap()).unwrap()
}
#[test]
fn fractional_units_preserve_subminute_work() {
    let mut p = base(&[1]);
    assign(&mut p, 1, 1, 3333);
    let r = report(&p);
    assert_eq!(r.resources[0].planned_basis, 3333);
    assert_eq!(work_decimal(3333), "0.3333");
    assert_eq!(r.task_work_basis[&tid(1)], 3333);
}
#[test]
fn overlapping_assignments_warn_without_moving_dates() {
    let mut p = base(&[120, 120]);
    let before = schedule(&p).unwrap();
    assign(&mut p, 1, 1, 10000);
    assign(&mut p, 2, 1, 10000);
    let s = schedule(&p).unwrap();
    assert_eq!(s.finish, before.finish);
    assert_eq!(s.tasks[1].early_start, before.tasks[1].early_start);
    let r = report(&p);
    let r = &r.resources[0];
    assert_eq!(r.peak_units, 20000);
    assert_eq!(r.overloaded_minutes, 120);
    assert_eq!(r.excess_basis, 120 * 10000);
    assert_eq!(r.overloads[0].task_ids, vec![tid(1), tid(2)]);
}
#[test]
fn half_assignments_fit_one_person() {
    let mut p = base(&[120, 120]);
    assign(&mut p, 1, 1, 5000);
    assign(&mut p, 2, 1, 5000);
    let r = report(&p);
    assert_eq!(r.resources[0].overload_count, 0);
    assert_eq!(r.resources[0].planned_basis, 120 * 10000);
}
#[test]
fn half_open_boundaries_do_not_create_false_conflicts() {
    let mut p = base(&[120, 120]);
    p.links.push(Dependency {
        from: tid(1),
        to: tid(2),
        relation: Relation::FS,
        lag: Lag::new(0).unwrap(),
    });
    assign(&mut p, 1, 1, 10000);
    assign(&mut p, 2, 1, 10000);
    let r = report(&p);
    assert_eq!(r.resources[0].overloaded_minutes, 0);
    assert_eq!(r.resources[0].peak_units, 10000);
}
#[test]
fn absence_keeps_demand_and_creates_full_overload() {
    let mut p = base(&[480]);
    p.resources[0].availability.push(Availability {
        from: date("2026-10-26"),
        through: date("2026-10-26"),
        capacity: u(0),
    });
    assign(&mut p, 1, 1, 10000);
    let r = report(&p);
    assert_eq!(r.resources[0].capacity_basis, 0);
    assert_eq!(r.resources[0].planned_basis, 480 * 10000);
    assert_eq!(r.resources[0].excess_basis, 480 * 10000);
    assert_eq!(r.resources[0].overloaded_minutes, 480);
}
#[test]
fn availability_period_is_inclusive_and_outside_period_uses_base() {
    let mut p = base(&[1440]);
    p.resources[0].availability.push(Availability {
        from: date("2026-10-27"),
        through: date("2026-10-27"),
        capacity: u(5000),
    });
    assign(&mut p, 1, 1, 10000);
    let r = report(&p);
    assert_eq!(r.resources[0].capacity_basis, 1200 * 10000);
    assert_eq!(r.resources[0].overloaded_minutes, 480);
    assert_eq!(r.resources[0].daily[1].capacity_basis, 240 * 10000);
}
#[test]
fn daily_average_does_not_hide_a_concurrent_peak() {
    let mut p = base(&[60, 60, 480]);
    assign(&mut p, 1, 1, 10000);
    assign(&mut p, 2, 1, 10000);
    let r = report(&p);
    let day = &r.resources[0].daily[0];
    assert_eq!(day.planned_basis, 120 * 10000);
    assert_eq!(day.capacity_basis, 480 * 10000);
    assert_eq!(day.overloaded_minutes, 60);
    assert_eq!(day.excess_basis, 60 * 10000);
}
#[test]
fn holiday_and_lunch_do_not_accrue_effort_or_capacity() {
    let mut p = base(&[600]);
    for day in &mut p.calendar.weekly[..5] {
        *day = calendar::intervals("09:00-13:00,14:00-18:00").unwrap();
    }
    p.calendar.holidays.insert(date("2026-10-27"));
    assign(&mut p, 1, 1, 5000);
    let r = report(&p);
    assert_eq!(r.resources[0].planned_basis, 300 * 10000);
    assert_eq!(r.resources[0].capacity_basis, 600 * 10000);
    assert_eq!(r.resources[0].daily[1].date, date("2026-10-28"));
}
#[test]
fn generic_team_capacity_and_multiple_resources_sum_work() {
    let mut p = base(&[60]);
    p.resources[0].kind = ResourceKind::Generic;
    p.resources[0].capacity = u(20000);
    p.resources.push(resource(2, 10000));
    assign(&mut p, 1, 1, 15000);
    assign(&mut p, 1, 2, 5000);
    let r = report(&p);
    assert_eq!(r.task_work_basis[&tid(1)], 120 * 10000);
    assert!(r.resources.iter().all(|r| r.overload_count == 0));
}
#[test]
fn invalid_ids_duplicate_pairs_summaries_and_overlapping_periods_fail() {
    let mut p = base(&[60]);
    assign(&mut p, 1, 99, 10000);
    assert!(schedule(&p).is_err());
    p.assignments.clear();
    assign(&mut p, 99, 1, 10000);
    assert!(schedule(&p).is_err());
    p.assignments.clear();
    assign(&mut p, 1, 1, 0);
    assert!(schedule(&p).is_err());
    p.assignments.clear();
    assign(&mut p, 1, 1, 10000);
    assign(&mut p, 1, 1, 5000);
    assert!(schedule(&p).is_err());
    p.assignments.pop();
    p.tasks[0].kind = TaskKind::Summary;
    p.tasks[0].duration = WorkDuration::new(0).unwrap();
    assert!(schedule(&p).is_err());
    let mut p = base(&[60]);
    p.resources[0].availability = vec![
        Availability {
            from: date("2026-10-26"),
            through: date("2026-10-28"),
            capacity: u(0),
        },
        Availability {
            from: date("2026-10-28"),
            through: date("2026-10-29"),
            capacity: u(5000),
        },
    ];
    assert!(schedule(&p).is_err());
    assert!(ResourceId::new(0).is_err());
    assert!(Units::new(1000001).is_err());
}
#[test]
fn future_availability_does_not_expand_the_schedule_horizon() {
    let mut p = base(&[60]);
    p.resources[0].availability.push(Availability {
        from: date("9999-01-01"),
        through: date("9999-12-31"),
        capacity: u(0),
    });
    assign(&mut p, 1, 1, 10000);
    assert_eq!(report(&p).resources[0].capacity_basis, 60 * 10000);
}
#[test]
fn resource_and_task_deletion_are_reversible_and_cleanup_assignments() {
    let mut p = base(&[60]);
    assign(&mut p, 1, 1, 10000);
    let mut e = Editor::new(p).unwrap();
    let original = serialize(e.project()).unwrap();
    e.apply(e.revision(), vec![Command::DeleteResource(rid(1))])
        .unwrap();
    assert!(e.project().assignments.is_empty());
    e.undo(e.revision()).unwrap();
    assert_eq!(serialize(e.project()).unwrap(), original);
    e.apply(e.revision(), vec![Command::DeleteSubtree { id: tid(1) }])
        .unwrap();
    assert!(e.project().assignments.is_empty());
    e.undo(e.revision()).unwrap();
    assert_eq!(serialize(e.project()).unwrap(), original);
}
#[test]
fn resource_update_and_assignment_batch_is_atomic() {
    let p = base(&[60]);
    let mut e = Editor::new(p).unwrap();
    let before = serialize(e.project()).unwrap();
    let commands = vec![
        Command::UpdateResource(resource(1, 5000)),
        Command::SetAssignments {
            task: tid(1),
            assignments: vec![Assignment {
                task: tid(1),
                resource: rid(99),
                units: u(10000),
            }],
        },
    ];
    assert!(e.apply(e.revision(), commands).is_err());
    assert_eq!(e.revision().get(), 0);
    assert_eq!(serialize(e.project()).unwrap(), before);
    assert!(!e.can_undo());
}
#[test]
fn resource_ids_do_not_reuse_after_undo() {
    let mut e = Editor::new(base(&[60])).unwrap();
    let next = e.next_resource_id().unwrap();
    e.apply(
        e.revision(),
        vec![Command::AddResource(resource(next.get(), 10000))],
    )
    .unwrap();
    e.undo(e.revision()).unwrap();
    assert!(e.next_resource_id().unwrap() > next);
    assert!(e
        .apply(
            e.revision(),
            vec![Command::AddResource(resource(next.get(), 10000))]
        )
        .is_err());
}
#[test]
fn summary_work_is_sum_not_duration() {
    let p = parse(include_str!("../../../examples/infrastructure.fplan")).unwrap();
    let r = report(&p);
    assert_eq!(
        r.task_work_basis[&tid(10)],
        (1440 + 2400 + 1440 + 1440) * 10000
    );
    assert_eq!(r.resources[0].overloaded_minutes, 1440);
    assert!(r.resources[0].planned_basis < r.resources[0].capacity_basis);
}
#[test]
fn v4_roundtrip_is_canonical_and_v1_v2_v3_migrate_without_resources() {
    let p = parse(include_str!("../../../examples/infrastructure.fplan")).unwrap();
    let out = serialize(&p).unwrap();
    assert!(out.contains("\"schema_version\": 1"));
    assert_eq!(serialize(&parse(&out).unwrap()).unwrap(), out);
    for text in [
        include_str!("../../../examples/legacy-v1.fplan"),
        include_str!("../../../examples/legacy-v2.fplan"),
        include_str!("../../../examples/legacy-v3.fplan"),
    ] {
        let p = parse(text).unwrap();
        assert!(p.resources.is_empty() && p.assignments.is_empty());
        assert_eq!(schedule(&p).unwrap().finish, 6720);
    }
}
#[test]
fn reporting_limits_are_explicit_and_totals_remain_complete() {
    let mut p = base(&[480 * 100]);
    assign(&mut p, 1, 1, 20000);
    let r = report(&p);
    let r = &r.resources[0];
    assert!(r.daily_truncated);
    assert_eq!(r.daily.len(), DAILY_LIMIT);
    assert_eq!(r.planned_basis, 96000 * 10000);
    assert_eq!(r.overloaded_minutes, 48000);
    let mut p = base(&[480 * 220]);
    assign(&mut p, 1, 1, 20000);
    let mut tl = Timeline::new(&p.calendar).unwrap();
    for n in 0..220 {
        let day = tl.start_at(n * 480).unwrap().local();
        p.resources[0].availability.push(Availability {
            from: date(&day[..10]),
            through: date(&day[..10]),
            capacity: u(if n % 2 == 0 { 5000 } else { 10000 }),
        });
    }
    let r = report(&p);
    assert!(r.resources[0].overload_count > OVERLOAD_LIMIT);
    assert_eq!(r.resources[0].overloads.len(), OVERLOAD_LIMIT);
}
#[test]
fn sweep_report_matches_independent_minute_oracle() {
    for seed in 0..20u32 {
        let mut p = base(&[9, 7, 11]);
        for n in 1..=3 {
            assign(&mut p, n, 1, 2500 + (seed + n) % 4 * 2500);
        }
        p.resources[0].capacity = u(5000 + (seed % 3) * 2500);
        let s = schedule(&p).unwrap();
        let mut planned = 0;
        let mut excess = 0;
        let mut overloaded = 0;
        for minute in 0..s.finish {
            let demand: i64 = p
                .assignments
                .iter()
                .filter(|a| {
                    let t = &s.tasks[(a.task.get() - 1) as usize];
                    t.early_start <= minute && minute < t.early_finish
                })
                .map(|a| i64::from(a.units.basis_points()))
                .sum();
            planned += demand;
            let over = (demand - i64::from(p.resources[0].capacity.basis_points())).max(0);
            excess += over;
            if over > 0 {
                overloaded += 1;
            }
        }
        let r = report(&p);
        assert_eq!(r.resources[0].planned_basis, planned);
        assert_eq!(r.resources[0].excess_basis, excess);
        assert_eq!(r.resources[0].overloaded_minutes, overloaded);
    }
}

#[test]
fn mismatched_schedule_returns_an_error_instead_of_indexing_missing_tasks() {
    let p = base(&[60]);
    let other = base(&[60, 30]);
    let s = schedule(&other).unwrap();
    assert!(analyze(&p, &s).is_err());
    assert_eq!(work_decimal(-3333), "-0.3333");
}
