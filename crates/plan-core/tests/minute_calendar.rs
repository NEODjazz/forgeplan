use plan_core::calendar::{intervals, parse_offset};
use plan_core::*;
fn id(n: u32) -> TaskId {
    TaskId::new(n).unwrap()
}
fn moment(s: &str) -> Moment {
    Moment::parse(s).unwrap()
}
fn base(durations: &[i64]) -> Project {
    Project {
        title: "Minute tests".into(),
        calendar: Calendar::weekdays(Date::parse("2026-10-23").unwrap()),
        tasks: durations
            .iter()
            .enumerate()
            .map(|(i, &n)| Task {
                id: id(i as u32 + 1),
                title: "T".into(),
                duration: WorkDuration::new(n).unwrap(),
                parent: None,
                order: i as u32,
                kind: TaskKind::Task,
                constraint: None,
                deadline: None,
            })
            .collect(),
        links: vec![],
        management: Management::default(),
        resources: vec![],
        assignments: vec![],
    }
}
fn rule(p: &mut Project, index: usize, kind: ConstraintType, at: &str) {
    p.tasks[index].constraint = Some(Constraint {
        kind,
        at: moment(at),
    });
}
fn link(p: &mut Project, from: u32, to: u32, relation: Relation, lag: i64) {
    p.links.push(Dependency {
        from: id(from),
        to: id(to),
        relation,
        lag: Lag::new(lag).unwrap(),
    });
}
#[test]
fn minute_precision_and_split_lunch() {
    let mut p = base(&[300]);
    p.calendar.weekly[4] = intervals("09:00-13:00,14:00-18:00").unwrap();
    let s = schedule(&p).unwrap();
    assert_eq!(s.tasks[0].start.local(), "2026-10-23T09:00");
    assert_eq!(s.tasks[0].finish.local(), "2026-10-23T15:00");
    assert!(s.tasks[0].reasons.iter().any(|r| r.contains("перерывы")));
}
#[test]
fn start_during_lunch_and_short_final_day() {
    let mut p = base(&[90]);
    p.calendar.start_minute = 13 * 60 + 15;
    p.calendar.weekly[4] = intervals("09:00-13:00,14:00-18:00").unwrap();
    let s = schedule(&p).unwrap();
    assert_eq!(s.tasks[0].start.local(), "2026-10-23T14:00");
    assert_eq!(s.tasks[0].finish.local(), "2026-10-23T15:30");
}
#[test]
fn holiday_and_working_weekend_exception() {
    let mut p = base(&[600]);
    p.calendar.exceptions.insert(
        Date::parse("2026-10-24").unwrap(),
        intervals("10:00-14:00").unwrap(),
    );
    p.calendar
        .holidays
        .insert(Date::parse("2026-10-24").unwrap());
    let s = schedule(&p).unwrap();
    assert_eq!(s.tasks[0].finish.local(), "2026-10-24T12:00");
    p.calendar.exceptions.clear();
    p.calendar
        .holidays
        .insert(Date::parse("2026-10-26").unwrap());
    assert_eq!(
        schedule(&p).unwrap().tasks[0].finish.local(),
        "2026-10-27T11:00"
    );
}
#[test]
fn overnight_shift_is_explicitly_split_at_midnight() {
    let mut p = base(&[240]);
    p.calendar.start_minute = 22 * 60;
    p.calendar.weekly[4] = intervals("22:00-24:00").unwrap();
    p.calendar.weekly[5] = intervals("00:00-02:00").unwrap();
    assert_eq!(
        schedule(&p).unwrap().tasks[0].finish.local(),
        "2026-10-24T02:00"
    );
    assert!(intervals("22:00-02:00").is_err());
}
#[test]
fn snet_and_fnet_project_to_working_instant() {
    let mut p = base(&[60]);
    rule(&mut p, 0, ConstraintType::SNET, "2026-10-24T12:00");
    assert_eq!(
        schedule(&p).unwrap().tasks[0].start.local(),
        "2026-10-26T09:00"
    );
    rule(&mut p, 0, ConstraintType::FNET, "2026-10-24T12:00");
    let s = schedule(&p).unwrap();
    assert_eq!(s.tasks[0].finish.local(), "2026-10-26T09:01");
    assert_eq!(s.tasks[0].start.local(), "2026-10-23T16:01");
}
#[test]
fn exact_start_and_finish() {
    let mut p = base(&[90]);
    rule(&mut p, 0, ConstraintType::MSO, "2026-10-26T10:15");
    let s = schedule(&p).unwrap();
    assert_eq!(s.tasks[0].start.local(), "2026-10-26T10:15");
    assert_eq!(s.tasks[0].finish.local(), "2026-10-26T11:45");
    assert_eq!(s.tasks[0].total_float, Some(0));
    rule(&mut p, 0, ConstraintType::MFO, "2026-10-26T10:15");
    let s = schedule(&p).unwrap();
    assert_eq!(s.tasks[0].start.local(), "2026-10-23T16:45");
    assert_eq!(s.tasks[0].finish.local(), "2026-10-26T10:15");
}
#[test]
fn impossible_exact_points_are_rejected() {
    let mut p = base(&[60]);
    for (kind, at) in [
        (ConstraintType::MSO, "2026-10-24T12:00"),
        (ConstraintType::MSO, "2026-10-23T17:00"),
        (ConstraintType::MFO, "2026-10-26T09:00"),
    ] {
        rule(&mut p, 0, kind, at);
        assert!(schedule(&p).is_err());
    }
}
#[test]
fn upper_bounds_and_dependency_conflicts_are_hard() {
    let mut p = base(&[480, 60]);
    link(&mut p, 1, 2, Relation::FS, 0);
    for kind in [
        ConstraintType::SNLT,
        ConstraintType::FNLT,
        ConstraintType::MSO,
        ConstraintType::MFO,
    ] {
        rule(&mut p, 1, kind, "2026-10-23T10:00");
        assert!(schedule(&p).unwrap_err().contains("cannot meet"));
    }
    rule(&mut p, 1, ConstraintType::SNLT, "2026-10-26T10:00");
    assert!(schedule(&p).is_ok());
}
#[test]
fn upper_bounds_limit_float_before_weekend() {
    let mut p = base(&[60, 960]);
    rule(&mut p, 0, ConstraintType::SNLT, "2026-10-25T12:00");
    let s = schedule(&p).unwrap();
    assert_eq!(s.tasks[0].late_start, Some(479));
    rule(&mut p, 0, ConstraintType::FNLT, "2026-10-25T12:00");
    assert_eq!(schedule(&p).unwrap().tasks[0].late_start, Some(420));
}
#[test]
fn deadline_is_warning_and_not_a_hard_bound() {
    let mut p = base(&[120]);
    let expected = schedule(&p).unwrap();
    p.tasks[0].deadline = Some(moment("2026-10-23T10:00"));
    let s = schedule(&p).unwrap();
    assert_eq!(s.tasks[0].start, expected.tasks[0].start);
    assert!(s.tasks[0].deadline_missed);
    assert!(s.tasks[0].reasons.iter().any(|r| r.contains("Дедлайн")));
}
#[test]
fn summary_rollup_and_driving_dependency_explanations() {
    let p = parse(include_str!("../../../examples/infrastructure.fplan")).unwrap();
    let s = schedule(&p).unwrap();
    assert_eq!(s.finish, 6720);
    let task = s.tasks.iter().find(|t| t.id == id(4)).unwrap();
    assert!(task.reasons.iter().any(|r| r.contains("задача 2 FS")));
    assert!(!task.reasons.iter().any(|r| r.contains("задача 3 FS")));
}
#[test]
fn sf_and_ff_do_not_compare_a_monday_start_to_a_friday_finish_as_equal() {
    let mut p = base(&[0, 60]);
    rule(&mut p, 0, ConstraintType::MSO, "2026-10-26T09:00");
    link(&mut p, 1, 2, Relation::SF, 0);
    let s = schedule(&p).unwrap();
    assert!(s.tasks[1].finish >= s.tasks[0].start);
    assert_eq!(s.tasks[1].finish.local(), "2026-10-26T09:01");
    p.links[0].relation = Relation::FF;
    assert!(schedule(&p).unwrap().tasks[1].finish >= schedule(&p).unwrap().tasks[0].finish);
}
#[test]
fn working_lag_and_lead_cross_breaks() {
    let mut p = base(&[480, 60]);
    link(&mut p, 1, 2, Relation::FS, 60);
    assert_eq!(
        schedule(&p).unwrap().tasks[1].start.local(),
        "2026-10-26T10:00"
    );
    p.links[0].lag = Lag::new(-60).unwrap();
    assert_eq!(
        schedule(&p).unwrap().tasks[1].start.local(),
        "2026-10-23T16:00"
    );
}
#[test]
fn rejected_date_batch_preserves_revision_and_undo() {
    let p = base(&[480, 60]);
    let mut e = Editor::new(p).unwrap();
    let before = serialize(e.project()).unwrap();
    let invalid = Command::Dates {
        id: id(1),
        constraint: Some(Constraint {
            kind: ConstraintType::MFO,
            at: moment("2026-10-23T10:00"),
        }),
        deadline: None,
    };
    assert!(e.apply(e.revision(), vec![invalid]).is_err());
    assert_eq!(e.revision().get(), 0);
    assert_eq!(serialize(e.project()).unwrap(), before);
    assert!(!e.can_undo());
}
#[test]
fn calendar_validation_and_offset_policy() {
    for hours in [
        "09:00-13:00,12:00-17:00",
        "17:00-09:00",
        "09:00-25:00",
        "09:00-09:00",
    ] {
        assert!(intervals(hours).is_err());
    }
    assert_eq!(parse_offset("+03:00").unwrap(), 180);
    for off in ["Europe/Moscow", "+14:30", "+25:00"] {
        assert!(parse_offset(off).is_err());
    }
    let mut p = base(&[1]);
    p.calendar.utc_offset = 180;
    assert!(schedule_json(&p, &schedule(&p).unwrap()).contains("2026-10-23T09:01+03:00"));
}
#[test]
fn migration_v1_v2_and_minute_v3_roundtrip() {
    for text in [
        include_str!("../../../examples/legacy-v1.fplan"),
        include_str!("../../../examples/legacy-v2.fplan"),
    ] {
        let p = parse(text).unwrap();
        assert_eq!(schedule(&p).unwrap().finish, 6720);
        let out = serialize(&p).unwrap();
        assert!(out.contains("\"schema_version\": 1"));
        assert_eq!(serialize(&parse(&out).unwrap()).unwrap(), out);
    }
    let mut p = base(&[37]);
    rule(&mut p, 0, ConstraintType::SNET, "2026-10-23T10:07");
    p.tasks[0].deadline = Some(moment("2026-10-23T12:00"));
    let out = serialize(&p).unwrap();
    assert_eq!(serialize(&parse(&out).unwrap()).unwrap(), out);
}
#[test]
fn inverse_late_pass_matches_bruteforce_across_calendar_gap() {
    for relation in [Relation::FS, Relation::SS, Relation::FF, Relation::SF] {
        for lag in -2..=2 {
            let mut p = base(&[1, 1]);
            p.calendar.weekly[4] = intervals("09:00-09:02,10:00-10:03").unwrap();
            link(&mut p, 1, 2, relation, lag);
            let s = schedule(&p).unwrap();
            let finish = s.finish;
            let mut latest = [0, 0];
            let mut tl = Timeline::new(&p.calendar).unwrap();
            for a in 0..=finish - 1 {
                for b in 0..=finish - 1 {
                    let anchor = if matches!(relation, Relation::FS | Relation::FF) {
                        tl.finish_at(a + 1).unwrap()
                    } else {
                        tl.start_at(a).unwrap()
                    };
                    let c =
                        a + if matches!(relation, Relation::FS | Relation::FF) {
                            1
                        } else {
                            0
                        } + lag;
                    let target = if c < 0 {
                        None
                    } else if lag > 0 {
                        Some(tl.finish_at(c).unwrap())
                    } else if lag < 0 {
                        Some(tl.start_at(c).unwrap())
                    } else {
                        Some(anchor)
                    };
                    let endpoint = if matches!(relation, Relation::FS | Relation::SS) {
                        tl.start_at(b).unwrap()
                    } else {
                        tl.finish_at(b + 1).unwrap()
                    };
                    if target.is_none_or(|t| endpoint >= t) {
                        latest[0] = latest[0].max(a);
                        latest[1] = latest[1].max(b);
                    }
                }
            }
            assert_eq!(s.tasks[0].late_start, Some(latest[0]));
            assert_eq!(s.tasks[1].late_start, Some(latest[1]));
        }
    }
}
