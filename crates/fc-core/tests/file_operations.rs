mod common;

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use common::*;
use fc_core::compare::{DiffEngine, DiffResult};
use fc_core::model::{CompareOptions, ContentCompareMode, DiffStatus, NodeId, Side};
use fc_core::ops::*;

#[derive(Default)]
struct FakeTrash {
    files: Mutex<Vec<PathBuf>>,
    dirs: Mutex<Vec<PathBuf>>,
}

impl Trash for FakeTrash {
    fn delete_file(&self, path: &Path) -> io::Result<()> {
        self.files.lock().unwrap().push(path.to_path_buf());
        Ok(())
    }
    fn delete_dir(&self, path: &Path) -> io::Result<()> {
        self.dirs.lock().unwrap().push(path.to_path_buf());
        Ok(())
    }
}

fn options(r: &DiffResult, policy: OverwritePolicy, recycle: bool) -> OperationOptions {
    OperationOptions {
        overwrite_policy: policy,
        delete_to_recycle_bin: recycle,
        ..OperationOptions::new(&r.left_root, &r.right_root)
    }
}

fn opts(r: &DiffResult) -> OperationOptions {
    options(r, OverwritePolicy::Overwrite, false)
}

fn run(plan: &OperationPlan, options: &OperationOptions) -> OperationSummary {
    FileOperationExecutor::new(&FakeTrash::default()).execute(
        &plan.actions,
        options,
        None,
        None,
        &AtomicBool::new(false),
    )
}

fn run_with(
    plan: &OperationPlan,
    options: &OperationOptions,
    resolver: &mut dyn FnMut(&OverwriteConflict) -> OverwriteDecision,
) -> OperationSummary {
    FileOperationExecutor::new(&FakeTrash::default()).execute(
        &plan.actions,
        options,
        Some(resolver),
        None,
        &AtomicBool::new(false),
    )
}

fn copy_plan(r: &DiffResult, rel: &str, dir: CopyDirection) -> OperationPlan {
    plan_copy(r, &[find_id(r, rel)], dir)
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap()
}

fn decision(choice: OverwriteChoice, apply_to_all: bool) -> OverwriteDecision {
    OverwriteDecision {
        choice,
        apply_to_all,
    }
}

#[test]
fn copy_preserves_timestamp_and_creates_parents() {
    let f = Fixture::new();
    add_file(&f.left(r"a\b\c.txt"), t0(), b"hello");
    let r = f.compare();

    let summary = run(&copy_plan(&r, "a", CopyDirection::LeftToRight), &opts(&r));

    assert_eq!(0, summary.failed());
    let target = f.right(r"a\b\c.txt");
    assert_eq!("hello", read(&target));
    assert_eq!(t0(), mtime(&target));
    let mut temp = target.into_os_string();
    temp.push(TEMP_SUFFIX);
    assert!(!Path::new(&temp).exists());
}

#[test]
fn copy_then_refresh_makes_identical() {
    let f = Fixture::new();
    add_file(&f.left(r"d\x.txt"), t0() + secs(7200), b"new");
    add_file(&f.right(r"d\x.txt"), t0(), b"old");
    let mut r = f.compare();
    let id = find_id(&r, r"d\x.txt");

    run(
        &copy_plan(&r, r"d\x.txt", CopyDirection::LeftToRight),
        &opts(&r),
    );
    let (parent, name) = (
        r.tree.node(id).parent.unwrap(),
        r.tree.node(id).name.clone(),
    );
    DiffEngine::new().refresh(&mut r, parent, &name).unwrap();

    assert_eq!(DiffStatus::Identical, find(&r, r"d\x.txt").status);
    assert_eq!(DiffStatus::Identical, r.tree.root().status);
    assert_eq!("new", read(&f.right(r"d\x.txt")));
}

#[test]
fn copy_right_to_left() {
    let f = Fixture::new();
    add_file(&f.right("only.txt"), t0(), b"r");
    let r = f.compare();
    run(
        &copy_plan(&r, "only.txt", CopyDirection::RightToLeft),
        &opts(&r),
    );
    assert_eq!("r", read(&f.left("only.txt")));
}

fn set_readonly(path: &Path) {
    let mut p = fs::metadata(path).unwrap().permissions();
    p.set_readonly(true);
    fs::set_permissions(path, p).unwrap();
}

#[test]
fn copy_preserves_read_only_and_overwrites_read_only_target() {
    let f = Fixture::new();
    add_file(&f.left("ro.txt"), t0() + secs(3600), b"new");
    set_readonly(&f.left("ro.txt"));
    add_file(&f.right("ro.txt"), t0(), b"old");
    set_readonly(&f.right("ro.txt"));
    let r = f.compare();

    let summary = run(
        &copy_plan(&r, "ro.txt", CopyDirection::LeftToRight),
        &opts(&r),
    );

    assert_eq!(1, summary.succeeded(), "{}", summary.to_report());
    assert_eq!("new", read(&f.right("ro.txt")));
    assert!(
        fs::metadata(f.right("ro.txt"))
            .unwrap()
            .permissions()
            .readonly()
    );
}

#[test]
fn copy_identical_files_inside_folder_are_not_copied() {
    let f = Fixture::new();
    add(&f.left(r"d\same.txt"), t0());
    add(&f.right(r"d\same.txt"), t0());
    add(&f.left(r"d\new.txt"), t0());
    let r = f.compare();
    let plan = copy_plan(&r, "d", CopyDirection::LeftToRight);
    assert_eq!(1, plan.actions.len());
    assert!(plan.actions[0].target_path.ends_with("new.txt"));
}

fn conflict() -> (Fixture, DiffResult) {
    let f = Fixture::new();
    add_file(&f.left("x.txt"), t0() + secs(3600), b"new");
    add_file(&f.right("x.txt"), t0(), b"old");
    let r = f.compare();
    (f, r)
}

fn same_time_conflict() -> (Fixture, DiffResult) {
    let f = Fixture::new();
    add_file(&f.left("x.txt"), t0(), b"left!");
    add_file(&f.right("x.txt"), t0(), b"right");
    let r = f.compare_with(&CompareOptions {
        content_compare: ContentCompareMode::WhenTimestampsEqual,
        ..CompareOptions::default()
    });
    (f, r)
}

#[test]
fn overwrite_skip() {
    let (f, r) = conflict();
    let s = run(
        &copy_plan(&r, "x.txt", CopyDirection::LeftToRight),
        &options(&r, OverwritePolicy::Skip, false),
    );
    assert_eq!(1, s.skipped());
    assert_eq!("old", read(&f.right("x.txt")));
}

#[test]
fn overwrite_if_newer_overwrites_older_target() {
    let (f, r) = conflict();
    let s = run(
        &copy_plan(&r, "x.txt", CopyDirection::LeftToRight),
        &options(&r, OverwritePolicy::OverwriteIfNewer, false),
    );
    assert_eq!(1, s.succeeded());
    assert_eq!("new", read(&f.right("x.txt")));
}

#[test]
fn overwrite_if_newer_skips_newer_target() {
    let (f, r) = conflict();
    let s = run(
        &copy_plan(&r, "x.txt", CopyDirection::RightToLeft),
        &options(&r, OverwritePolicy::OverwriteIfNewer, false),
    );
    assert_eq!(1, s.skipped());
    assert_eq!("new", read(&f.left("x.txt")));
}

#[test]
fn overwrite_keep_both() {
    let (f, r) = conflict();
    let s = run(
        &copy_plan(&r, "x.txt", CopyDirection::LeftToRight),
        &options(&r, OverwritePolicy::KeepBoth, false),
    );
    assert_eq!(1, s.succeeded());
    assert_eq!("old", read(&f.right("x.txt")));
    assert_eq!("new", read(&f.right("x (1).txt")));
    assert_eq!(Some(f.right("x (1).txt")), s.items[0].final_target);
}

#[test]
fn overwrite_ask_uses_resolver_and_apply_to_all() {
    let f = Fixture::new();
    for i in 0..3 {
        add_file(&f.left(&format!("f{i}.txt")), t0() + secs(3600), b"new");
        add_file(&f.right(&format!("f{i}.txt")), t0(), b"old");
    }
    let r = f.compare();
    let mut asked = 0;
    let all: Vec<NodeId> = r.tree.root().children.clone();
    let s = run_with(
        &plan_copy(&r, &all, CopyDirection::LeftToRight),
        &options(&r, OverwritePolicy::Ask, false),
        &mut |_| {
            asked += 1;
            decision(OverwriteChoice::Overwrite, true)
        },
    );
    assert_eq!(1, asked);
    assert_eq!(3, s.succeeded());
}

#[test]
fn overwrite_newer_target_always_asks_even_with_overwrite_policy() {
    let (f, r) = conflict();
    let mut seen = None;
    let s = run_with(
        &copy_plan(&r, "x.txt", CopyDirection::RightToLeft),
        &options(&r, OverwritePolicy::Overwrite, false),
        &mut |c| {
            seen = Some(c.clone());
            decision(OverwriteChoice::Skip, false)
        },
    );
    assert!(seen.unwrap().target_is_newer);
    assert_eq!(1, s.skipped());
    assert_eq!("new", read(&f.left("x.txt")));
}

#[test]
fn same_time_conflict_planner_does_not_pick_direction() {
    let (_f, r) = same_time_conflict();
    assert_eq!(
        DiffStatus::SameTimeDifferentContent,
        find(&r, "x.txt").status
    );
    for dir in [CopyDirection::LeftToRight, CopyDirection::RightToLeft] {
        let plan = copy_plan(&r, "x.txt", dir);
        assert_eq!(1, plan.actions.len());
        assert!(plan.actions[0].direction_unknown);
        assert!(!plan.actions[0].target_is_newer);
    }
}

#[test]
fn same_time_conflict_is_not_overwritten_without_asking() {
    for policy in [
        OverwritePolicy::Overwrite,
        OverwritePolicy::OverwriteIfNewer,
    ] {
        let (f, r) = same_time_conflict();
        let s = run(
            &copy_plan(&r, "x.txt", CopyDirection::LeftToRight),
            &options(&r, policy, false),
        );
        assert_eq!(1, s.skipped());
        assert_eq!(
            Some("Same timestamp but different content; direction unknown."),
            s.items[0].message.as_deref()
        );
        assert_eq!("right", read(&f.right("x.txt")));
    }
}

#[test]
fn same_time_conflict_asks_even_with_overwrite_policy() {
    let (f, r) = same_time_conflict();
    let mut seen = None;
    let s = run_with(
        &copy_plan(&r, "x.txt", CopyDirection::LeftToRight),
        &options(&r, OverwritePolicy::Overwrite, false),
        &mut |c| {
            seen = Some(c.clone());
            decision(OverwriteChoice::Overwrite, false)
        },
    );
    let seen = seen.unwrap();
    assert!(seen.direction_unknown);
    assert!(!seen.target_is_newer);
    assert_eq!(1, s.succeeded());
    assert_eq!("left!", read(&f.right("x.txt")));
}

#[test]
fn same_time_conflict_overwrite_if_newer_choice_skips() {
    let (f, r) = same_time_conflict();
    let s = run_with(
        &copy_plan(&r, "x.txt", CopyDirection::LeftToRight),
        &options(&r, OverwritePolicy::Ask, false),
        &mut |_| decision(OverwriteChoice::OverwriteIfNewer, false),
    );
    assert_eq!(1, s.skipped());
    assert_eq!("right", read(&f.right("x.txt")));
}

#[test]
fn same_content_different_time_is_not_copied() {
    let f = Fixture::new();
    add_file(&f.left("x.txt"), t0() + secs(3 * 3600), b"same");
    add_file(&f.right("x.txt"), t0(), b"same");
    let r = f.compare_with(&CompareOptions {
        content_compare: ContentCompareMode::WhenSizesEqual,
        ..CompareOptions::default()
    });
    let plan = copy_plan(&r, "x.txt", CopyDirection::LeftToRight);
    assert!(plan.actions.is_empty());
    assert!(
        plan.notes
            .iter()
            .any(|n| n.contains("only the timestamps differ"))
    );
}

#[test]
fn plan_copy_notes() {
    let f = Fixture::new();
    add(&f.left("same.txt"), t0());
    add(&f.right("same.txt"), t0());
    add(&f.right("ronly.txt"), t0());
    add(&f.left("conflict"), t0());
    add(&f.right(r"conflict\x.txt"), t0());
    let r = f.compare();
    let ids: Vec<NodeId> = ["same.txt", "ronly.txt", "conflict"]
        .iter()
        .map(|n| find_id(&r, n))
        .collect();
    let plan = plan_copy(&r, &ids, CopyDirection::LeftToRight);
    assert!(plan.actions.is_empty());
    assert_eq!(
        vec![
            "same.txt: already identical.".to_string(),
            "ronly.txt: does not exist on the source side.".to_string(),
            "conflict: skipped, file on one side and folder on the other.".to_string(),
        ],
        plan.notes
    );
}

#[test]
fn overwrite_cancel_from_resolver_stops_queue() {
    let (f, r) = conflict();
    let s = run_with(
        &copy_plan(&r, "x.txt", CopyDirection::LeftToRight),
        &options(&r, OverwritePolicy::Ask, false),
        &mut |_| decision(OverwriteChoice::Cancel, false),
    );
    assert!(s.cancelled);
    assert_eq!("old", read(&f.right("x.txt")));
}

#[test]
fn verification_fails_when_timestamp_not_preserved() {
    let f = Fixture::new();
    add_file(&f.left("a.txt"), t0(), b"x");
    let r = f.compare();
    let trash = FakeTrash::default();
    let mut ex = FileOperationExecutor::new(&trash);
    ex.set_times = false;
    let s = ex.execute(
        &copy_plan(&r, "a.txt", CopyDirection::LeftToRight).actions,
        &opts(&r),
        None,
        None,
        &AtomicBool::new(false),
    );
    assert_eq!(1, s.items.len());
    assert_eq!(ItemOutcome::Failed, s.items[0].outcome);
    assert!(s.items[0].message.as_deref().unwrap().contains("timestamp"));
}

#[test]
fn path_traversal_is_rejected() {
    let f = Fixture::new();
    add(&f.left("a.txt"), t0());
    let r = f.compare();
    let mut evil = copy_plan(&r, "a.txt", CopyDirection::LeftToRight).actions[0].clone();
    evil.target_path = f.r.join(p(r"..\outside\a.txt"));
    let s = FileOperationExecutor::new(&FakeTrash::default()).execute(
        &[evil],
        &opts(&r),
        None,
        None,
        &AtomicBool::new(false),
    );
    assert_eq!(ItemOutcome::Failed, s.items[0].outcome);
    assert!(
        s.items[0]
            .message
            .as_deref()
            .unwrap()
            .starts_with("Refusing to operate on")
    );
    assert!(!f.dir.path().join("outside").exists());
}

#[test]
fn delete_of_root_is_rejected() {
    let f = Fixture::new();
    add(&f.left("a.txt"), t0());
    let r = f.compare();
    let mut action = plan_delete(&r, &[find_id(&r, "a.txt")], Sides::Left).actions[0].clone();
    action.target_path = f.l.clone();
    action.is_dir = true;
    let s = FileOperationExecutor::new(&FakeTrash::default()).execute(
        &[action],
        &opts(&r),
        None,
        None,
        &AtomicBool::new(false),
    );
    assert_eq!(ItemOutcome::Failed, s.items[0].outcome);
    assert!(f.l.is_dir());
}

#[test]
fn delete_permanent() {
    let f = Fixture::new();
    add(&f.left("a.txt"), t0());
    add(&f.left(r"d\b.txt"), t0());
    set_readonly(&f.left(r"d\b.txt"));
    add(&f.right("a.txt"), t0());
    let r = f.compare();

    let s = run(
        &plan_delete(&r, &[find_id(&r, "a.txt"), find_id(&r, "d")], Sides::Left),
        &opts(&r),
    );

    assert_eq!(2, s.succeeded(), "{}", s.to_report());
    assert!(!f.left("a.txt").exists());
    assert!(!f.left("d").exists());
    assert!(f.right("a.txt").exists());
}

#[test]
fn delete_to_recycle_bin_uses_trash() {
    let f = Fixture::new();
    add(&f.left("a.txt"), t0());
    add(&f.right("a.txt"), t0());
    add(&f.left(r"d\x.txt"), t0());
    let r = f.compare();
    let trash = FakeTrash::default();
    let plan = plan_delete(&r, &[find_id(&r, "a.txt"), find_id(&r, "d")], Sides::Both);
    FileOperationExecutor::new(&trash).execute(
        &plan.actions,
        &options(&r, OverwritePolicy::Ask, true),
        None,
        None,
        &AtomicBool::new(false),
    );
    assert_eq!(
        vec![f.left("a.txt"), f.right("a.txt")],
        *trash.files.lock().unwrap()
    );
    assert_eq!(vec![f.left("d")], *trash.dirs.lock().unwrap());
    assert!(f.left("a.txt").exists()); // the fake trash does not touch the file system
}

#[test]
fn delete_missing_is_skipped() {
    let f = Fixture::new();
    add(&f.left("a.txt"), t0());
    let r = f.compare();
    fs::remove_file(f.left("a.txt")).unwrap();
    let s = run(
        &plan_delete(&r, &[find_id(&r, "a.txt")], Sides::Left),
        &opts(&r),
    );
    assert_eq!(1, s.skipped());
    assert_eq!(
        Some("File no longer exists."),
        s.items[0].message.as_deref()
    );
}

#[test]
fn plan_delete_skips_ancestor_duplicates() {
    let f = Fixture::new();
    add(&f.left(r"d\a.txt"), t0());
    let r = f.compare();
    let plan = plan_delete(
        &r,
        &[find_id(&r, r"d\a.txt"), find_id(&r, "d")],
        Sides::Left,
    );
    assert_eq!(1, plan.actions.len());
    assert!(plan.actions[0].is_dir);
    assert_eq!(4, plan.total_bytes());
}

#[test]
fn plan_delete_notes_missing_side() {
    let f = Fixture::new();
    add(&f.left("a.txt"), t0());
    let r = f.compare();
    let plan = plan_delete(&r, &[find_id(&r, "a.txt")], Sides::Right);
    assert!(plan.actions.is_empty());
    assert_eq!(
        vec!["a.txt: does not exist on the chosen side.".to_string()],
        plan.notes
    );
}

#[test]
fn copy_pre_cancelled_does_nothing() {
    let f = Fixture::new();
    add(&f.left("a.txt"), t0());
    let r = f.compare();
    let s = FileOperationExecutor::new(&FakeTrash::default()).execute(
        &copy_plan(&r, "a.txt", CopyDirection::LeftToRight).actions,
        &opts(&r),
        None,
        None,
        &AtomicBool::new(true),
    );
    assert!(s.cancelled);
    assert_eq!(Some("Cancelled"), s.items[0].message.as_deref());
    assert!(!f.right("a.txt").exists());
}

#[test]
fn copy_cancel_mid_file_leaves_no_partial_file() {
    let f = Fixture::new();
    add_file(&f.left("big.bin"), t0(), &vec![0u8; 40 * 1024 * 1024]);
    let r = f.compare();
    let cancel = AtomicBool::new(false);
    let mut progress = |p: &OperationProgress| {
        if p.file_bytes_done > 0 && p.file_bytes_done < p.file_bytes_total {
            cancel.store(true, Ordering::Relaxed);
        }
    };
    let s = FileOperationExecutor::new(&FakeTrash::default()).execute(
        &copy_plan(&r, "big.bin", CopyDirection::LeftToRight).actions,
        &opts(&r),
        None,
        Some(&mut progress),
        &cancel,
    );
    assert!(s.cancelled);
    assert!(!f.right("big.bin").exists());
    assert_eq!(0, fs::read_dir(&f.r).unwrap().count(), "no temp file left");
}

#[test]
fn rename_renames_within_folder() {
    let f = Fixture::new();
    add(&f.left(r"d\a.txt"), t0());
    let r = f.compare();
    let trash = FakeTrash::default();
    let ex = FileOperationExecutor::new(&trash);
    let new = ex
        .rename(&opts(&r), Side::Left, &p(r"d\a.txt"), "b.txt")
        .unwrap();
    assert_eq!(f.left(r"d\b.txt"), new);
    assert!(f.left(r"d\b.txt").exists());
    assert!(matches!(
        ex.rename(&opts(&r), Side::Left, &p(r"d\b.txt"), &p(r"..\..\evil.txt")),
        Err(OpError::InvalidName(_))
    ));
    assert_eq!(
        "'' is not a valid name.",
        ex.rename(&opts(&r), Side::Left, &p(r"d\b.txt"), "")
            .unwrap_err()
            .to_string()
    );
}

#[test]
fn rename_never_overwrites() {
    let f = Fixture::new();
    add_file(&f.left("a.txt"), t0(), b"a");
    add_file(&f.left("b.txt"), t0(), b"b");
    let r = f.compare();
    let trash = FakeTrash::default();
    let ex = FileOperationExecutor::new(&trash);
    assert!(ex.rename(&opts(&r), Side::Left, "a.txt", "b.txt").is_err());
    assert_eq!("b", read(&f.left("b.txt")));
}

#[test]
fn rename_case_only() {
    let f = Fixture::new();
    add(&f.left("a.txt"), t0());
    let r = f.compare();
    let trash = FakeTrash::default();
    FileOperationExecutor::new(&trash)
        .rename(&opts(&r), Side::Left, "a.txt", "A.txt")
        .unwrap();
    let names: Vec<String> = fs::read_dir(&f.l)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(vec!["A.txt".to_string()], names);
}

#[test]
fn summary_report() {
    let (_f, r) = conflict();
    let s = run(
        &copy_plan(&r, "x.txt", CopyDirection::LeftToRight),
        &options(&r, OverwritePolicy::Skip, false),
    );
    let nl = fc_core::NEWLINE;
    let report = s.to_report();
    assert!(report.starts_with(&format!(
        "Succeeded: 0   Skipped: 1   Failed: 0{nl}{nl}Skipped:{nl}  Copy "
    )));
    assert!(report.ends_with(&format!(" - Destination exists.{nl}")));
}
