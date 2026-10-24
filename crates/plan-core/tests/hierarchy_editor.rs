use plan_core::*;
fn id(n: u32) -> TaskId {
    TaskId::new(n).unwrap()
}
fn d(n: i64) -> WorkDuration {
    WorkDuration::new(n).unwrap()
}
fn initial() -> Project {
    parse(include_str!("../../../examples/infrastructure.fplan")).unwrap()
}
fn t(p: &Project, n: u32) -> &Task {
    p.tasks.iter().find(|t| t.id == id(n)).unwrap()
}
fn result(p: &Project, n: u32) -> ScheduledTask {
    schedule(p)
        .unwrap()
        .tasks
        .into_iter()
        .find(|t| t.id == id(n))
        .unwrap()
}
fn apply(e: &mut Editor, commands: Vec<Command>) {
    e.apply(e.revision(), commands).unwrap()
}
fn edit(n: u32, days: i64) -> Command {
    Command::Update {
        id: id(n),
        title: format!("Task {n}"),
        duration: d(days),
    }
}
fn add(n: u32, parent: Option<u32>, kind: TaskKind, days: i64) -> Command {
    Command::Add {
        task: Task {
            id: id(n),
            title: format!("Task {n}"),
            duration: d(days),
            parent: parent.map(id),
            order: 0,
            kind,
            constraint: None,
            deadline: None,
        },
        before: None,
    }
}
#[test]
fn rollup_is_span_not_sum() {
    let p = initial();
    let s = result(&p, 10);
    assert_eq!((s.early_start, s.early_finish), (0, 3840));
    assert_eq!(s.total_float, None);
    assert_eq!(s.late_start, None);
    assert!(!s.critical());
    let rows = outline(&p).unwrap();
    assert_eq!(
        rows.iter().map(|r| r.wbs.as_str()).collect::<Vec<_>>(),
        vec!["1", "1.1", "1.2", "1.3", "2", "2.1", "2.2", "2.3"]
    );
}
#[test]
fn typed_boundaries_reject_invalid_values() {
    assert!(TaskId::new(0).is_err());
    assert!(WorkDuration::new(-1).is_err());
    assert!(WorkDuration::new(MAX_HORIZON + 1).is_err());
    assert!(Lag::new(-MAX_HORIZON - 1).is_err());
}
#[test]
fn five_level_rollup_and_empty_summary() {
    let mut e = Editor::new(initial()).unwrap();
    apply(
        &mut e,
        vec![
            add(30, None, TaskKind::Summary, 0),
            add(31, Some(30), TaskKind::Summary, 0),
            add(32, Some(31), TaskKind::Summary, 0),
            add(33, Some(32), TaskKind::Summary, 0),
            add(34, Some(33), TaskKind::Task, 7),
            add(35, Some(31), TaskKind::Summary, 0),
        ],
    );
    for n in [30, 31, 32, 33] {
        let r = result(e.project(), n);
        assert_eq!((r.early_start, r.early_finish), (0, 7));
    }
    assert_eq!(result(e.project(), 35).early_finish, 0);
    assert_eq!(
        outline(e.project())
            .unwrap()
            .iter()
            .find(|r| e.project().tasks[r.index].id == id(34))
            .unwrap()
            .depth,
        4
    );
}
#[test]
fn hierarchy_validation_rejects_cycles_missing_parents_leaf_parents_and_order_ties() {
    let mut p = initial();
    p.tasks.iter_mut().find(|t| t.id == id(10)).unwrap().parent = Some(id(20));
    p.tasks.iter_mut().find(|t| t.id == id(20)).unwrap().parent = Some(id(10));
    assert!(schedule(&p).is_err());
    let mut p = initial();
    p.tasks[1].parent = Some(id(999));
    assert!(schedule(&p).is_err());
    let mut p = initial();
    p.tasks[1].parent = Some(id(2));
    assert!(schedule(&p).is_err());
    let mut p = initial();
    p.tasks[2].order = p.tasks[1].order;
    assert!(schedule(&p).is_err());
    let mut p = initial();
    p.tasks[0].duration = d(1);
    assert!(schedule(&p).is_err());
}
#[test]
fn hierarchy_depth_is_bounded() {
    let mut p = initial();
    for n in 100..165 {
        p.tasks.push(Task {
            id: id(n),
            title: "nested".into(),
            duration: d(0),
            parent: if n == 100 { None } else { Some(id(n - 1)) },
            order: if n == 100 { 2 } else { 0 },
            kind: TaskKind::Summary,
            constraint: None,
            deadline: None,
        });
    }
    assert!(schedule(&p).unwrap_err().contains("depth"));
}
#[test]
fn summary_links_are_rejected() {
    let mut p = initial();
    p.links.push(Dependency {
        from: id(10),
        to: id(4),
        relation: Relation::FS,
        lag: Lag::new(0).unwrap(),
    });
    assert!(schedule(&p).unwrap_err().contains("summary"));
}
#[test]
fn reorder_move_and_delete_preserve_ids_and_restore_links() {
    let mut e = Editor::new(initial()).unwrap();
    let before = serialize(e.project()).unwrap();
    apply(
        &mut e,
        vec![Command::Move {
            id: id(20),
            parent: None,
            before: Some(id(10)),
        }],
    );
    assert_eq!(
        outline(e.project()).unwrap()[0].index,
        e.project()
            .tasks
            .iter()
            .position(|t| t.id == id(20))
            .unwrap()
    );
    assert_eq!(t(e.project(), 4).parent, Some(id(20)));
    e.undo(e.revision()).unwrap();
    assert_eq!(serialize(e.project()).unwrap(), before);
    apply(&mut e, vec![Command::DeleteSubtree { id: id(10) }]);
    assert_eq!(e.project().tasks.len(), 4);
    assert!(!e
        .project()
        .links
        .iter()
        .any(|l| l.from == id(1) || l.to == id(2) || l.to == id(3)));
    e.undo(e.revision()).unwrap();
    assert_eq!(serialize(e.project()).unwrap(), before);
}
#[test]
fn atomic_batch_and_failed_command_preserve_history_and_revision() {
    let mut e = Editor::new(initial()).unwrap();
    apply(&mut e, vec![edit(2, 8)]);
    e.undo(e.revision()).unwrap();
    let before = serialize(e.project()).unwrap();
    let rev = e.revision();
    let bad = vec![
        edit(2, 9),
        Command::Move {
            id: id(10),
            parent: Some(id(10)),
            before: None,
        },
    ];
    assert!(e.apply(rev, bad).is_err());
    assert_eq!(e.revision(), rev);
    assert_eq!(serialize(e.project()).unwrap(), before);
    assert!(e.can_redo());
    e.redo(rev).unwrap();
    assert_eq!(t(e.project(), 2).duration.minutes(), 8);
}
#[test]
fn atomic_batch_is_one_undo_and_revisions_are_monotonic() {
    let mut e = Editor::new(initial()).unwrap();
    let before = serialize(e.project()).unwrap();
    apply(&mut e, vec![edit(1, 4), edit(2, 8)]);
    assert_eq!(e.revision().get(), 1);
    e.undo(e.revision()).unwrap();
    assert_eq!(e.revision().get(), 2);
    assert_eq!(serialize(e.project()).unwrap(), before);
    assert!(!e.can_undo());
    e.redo(e.revision()).unwrap();
    assert_eq!(e.revision().get(), 3);
    assert_eq!(t(e.project(), 2).duration.minutes(), 8);
}
#[test]
fn stale_revision_rejected_and_new_branch_clears_redo() {
    let mut e = Editor::new(initial()).unwrap();
    let stale = e.revision();
    apply(&mut e, vec![edit(2, 8)]);
    assert!(e
        .apply(stale, vec![edit(2, 99)])
        .unwrap_err()
        .contains("Revision conflict"));
    assert_eq!(t(e.project(), 2).duration.minutes(), 8);
    e.undo(e.revision()).unwrap();
    apply(&mut e, vec![edit(2, 7)]);
    assert!(!e.can_redo());
}
#[test]
fn moving_subtree_rejects_cycles_and_invalid_before() {
    let mut e = Editor::new(initial()).unwrap();
    apply(&mut e, vec![add(30, Some(10), TaskKind::Summary, 0)]);
    let before = serialize(e.project()).unwrap();
    assert!(e
        .apply(
            e.revision(),
            vec![Command::Move {
                id: id(10),
                parent: Some(id(30)),
                before: None
            }]
        )
        .is_err());
    assert!(e
        .apply(
            e.revision(),
            vec![Command::Move {
                id: id(1),
                parent: Some(id(20)),
                before: Some(id(2))
            }]
        )
        .is_err());
    assert_eq!(serialize(e.project()).unwrap(), before);
    apply(
        &mut e,
        vec![Command::Move {
            id: id(10),
            parent: Some(id(20)),
            before: Some(id(4)),
        }],
    );
    assert_eq!(t(e.project(), 1).parent, Some(id(10)));
}
#[test]
fn ids_are_not_reused_after_undo_or_delete() {
    let mut e = Editor::new(initial()).unwrap();
    let next = e.next_id().unwrap().get();
    apply(&mut e, vec![add(next, None, TaskKind::Task, 1)]);
    e.undo(e.revision()).unwrap();
    assert!(e.next_id().unwrap().get() > next);
    assert!(e
        .apply(e.revision(), vec![add(next, None, TaskKind::Task, 1)])
        .is_err());
}
#[test]
fn history_limit_and_noop() {
    let mut e = Editor::new(initial()).unwrap();
    let p = e.project().clone();
    apply(&mut e, vec![Command::Replace(Box::new(p))]);
    assert_eq!(e.revision().get(), 0);
    assert!(!e.can_undo());
    for n in 1..=55 {
        apply(&mut e, vec![edit(2, n)]);
    }
    let mut count = 0;
    while e.can_undo() {
        e.undo(e.revision()).unwrap();
        count += 1;
    }
    assert_eq!(count, 50);
}
#[test]
fn v1_migration_and_v2_canonical_roundtrip() {
    let p = parse(include_str!("../../../examples/legacy-v1.fplan")).unwrap();
    assert!(p
        .tasks
        .iter()
        .all(|t| t.parent.is_none() && t.kind == TaskKind::Task));
    assert_eq!(schedule(&p).unwrap().finish, 6720);
    let text = serialize(&p).unwrap();
    assert!(text.contains("\"schema_version\": 1"));
    assert_eq!(serialize(&parse(&text).unwrap()).unwrap(), text);
    let p = initial();
    let mut reverse = p.clone();
    reverse.tasks.reverse();
    assert_eq!(serialize(&reverse).unwrap(), serialize(&p).unwrap());
}
