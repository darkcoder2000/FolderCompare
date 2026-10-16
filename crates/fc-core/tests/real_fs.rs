//! Integration tests against the file system of the host OS (long paths, locked files, Unix modes, case).
mod common;

use std::fs;
use std::io;
use std::path::Path;
use std::sync::atomic::AtomicBool;

use common::*;
use fc_core::model::{DiffStatus, NodeId};
use fc_core::ops::*;
use fc_core::report;

struct NoTrash;

impl Trash for NoTrash {
    fn delete_file(&self, _: &Path) -> io::Result<()> {
        unreachable!()
    }
    fn delete_dir(&self, _: &Path) -> io::Result<()> {
        unreachable!()
    }
}

fn copy_all(r: &fc_core::compare::DiffResult) -> OperationSummary {
    let all: Vec<NodeId> = r.tree.root().children.clone();
    let plan = plan_copy(r, &all, CopyDirection::LeftToRight);
    FileOperationExecutor::new(&NoTrash).execute(
        &plan.actions,
        &OperationOptions::new(&r.left_root, &r.right_root),
        None,
        None,
        &AtomicBool::new(false),
    )
}

#[test]
fn long_paths_and_unicode_are_scanned_and_copied() {
    let f = Fixture::new();
    let deep: Vec<String> = (0..12)
        .map(|i| format!("folder_{i}_{}", "x".repeat(20)))
        .collect();
    let rel = Path::new(&deep.join(std::path::MAIN_SEPARATOR_STR)).join("Ünïcödé 文件.txt");
    let source = f.l.join(&rel);
    assert!(source.to_string_lossy().len() > 260);
    add_file(&source, t0(), b"long");

    let r = f.compare();
    let node = r
        .tree
        .descendants(NodeId::ROOT)
        .into_iter()
        .map(|id| r.tree.node(id))
        .find(|n| n.name == "Ünïcödé 文件.txt")
        .unwrap();
    assert_eq!(DiffStatus::OnlyLeft, node.status);

    let summary = copy_all(&r);
    assert_eq!(0, summary.failed(), "{}", summary.to_report());
    assert_eq!(t0(), mtime(&f.r.join(&rel)));
}

#[cfg(windows)]
#[test]
fn locked_source_is_reported_as_failure_and_queue_continues() {
    use std::os::windows::fs::OpenOptionsExt;
    let f = Fixture::new();
    fs::write(f.left("locked.txt"), "a").unwrap();
    fs::write(f.left("free.txt"), "b").unwrap();
    let r = f.compare();

    let summary = {
        let _lock = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .share_mode(0)
            .open(f.left("locked.txt"))
            .unwrap();
        copy_all(&r)
    };

    assert_eq!(1, summary.failed());
    assert_eq!(1, summary.succeeded());
    assert!(!f.right("locked.txt").exists());
    assert!(!f.right(&format!("locked.txt{TEMP_SUFFIX}")).exists());
    assert!(f.right("free.txt").exists());
}

#[cfg(unix)]
#[test]
fn unix_file_mode_is_copied() {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new();
    let source = f.left("run.sh");
    fs::write(&source, "#!/bin/sh").unwrap();
    fs::set_permissions(&source, fs::Permissions::from_mode(0o544)).unwrap();

    let r = f.compare();
    assert_eq!(
        Some(0o544),
        find(&r, "run.sh").left.as_ref().unwrap().unix_mode
    );
    let summary = copy_all(&r);

    assert_eq!(1, summary.succeeded(), "{}", summary.to_report());
    let mode = fs::metadata(f.right("run.sh"))
        .unwrap()
        .permissions()
        .mode()
        & 0o7777;
    assert_eq!(0o544, mode);
}

#[cfg(target_os = "linux")]
#[test]
fn case_only_names_on_linux_are_separate_files() {
    let f = Fixture::new();
    fs::write(f.left("a.txt"), "lower").unwrap();
    fs::write(f.left("A.txt"), "upper").unwrap();

    let r = f.compare();
    assert!(!r.paths().is_ignore_case());
    assert_eq!(2, r.tree.root().children.len());

    let summary = copy_all(&r);
    assert_eq!(2, summary.succeeded());
    assert_eq!("upper", fs::read_to_string(f.right("A.txt")).unwrap());
    assert_eq!("lower", fs::read_to_string(f.right("a.txt")).unwrap());
}

#[test]
fn report_csv_and_text() {
    let f = Fixture::new();
    add_file(&f.left(r"d\a,b.txt"), t0(), b"12345");
    add_file(&f.right(r"d\a,b.txt"), t0() + secs(60), b"123");
    let r = f.compare();
    let nodes = r.tree.descendants(NodeId::ROOT);
    let nl = fc_core::NEWLINE;
    let date = report::format_date(t0());
    let later = report::format_date(t0() + secs(60));

    let csv = report::write_csv(&r, &nodes);
    let lines: Vec<&str> = csv.split(nl).collect();
    assert_eq!(
        "RelativePath,Type,Status,LeftSize,LeftModified,RightSize,RightModified,Message",
        lines[0]
    );
    assert!(lines[1].starts_with("d,Folder,Differs,5,"));
    assert_eq!(
        format!(
            "\"{}\",File,NewerRight,5,{date},3,{later},",
            p(r"d\a,b.txt")
        ),
        lines[2]
    );

    let text = report::write_text(&r, &nodes, t0());
    let lines: Vec<&str> = text.split(nl).collect();
    assert_eq!("FolderCompare comparison report", lines[0]);
    assert_eq!(format!("Left:  {}", r.left_root.display()), lines[1]);
    assert_eq!(format!("Created: {date}"), lines[3]);
    assert_eq!("", lines[4]);
    assert!(lines[5].starts_with(&format!(
        "Differs                  d{}  L: ",
        std::path::MAIN_SEPARATOR
    )));
    assert_eq!(
        format!(
            "NewerRight               {}  L: {date}  R: {later}",
            p(r"d\a,b.txt")
        ),
        lines[6]
    );
}
