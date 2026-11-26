mod common;

use std::fs;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;

use common::*;
use fc_core::compare::{CompareError, DiffEngine, PathComparison};
use fc_core::model::{
    CompareOptions, ContentCompareMode, DiffStatus, NodeId, ScanProgress, SymlinkMode,
};

fn content(mode: ContentCompareMode) -> CompareOptions {
    CompareOptions {
        content_compare: mode,
        ..CompareOptions::default()
    }
}

#[test]
fn classifies_all_file_statuses() {
    let f = Fixture::new();
    add(&f.left("same.txt"), t0());
    add(&f.right("same.txt"), t0() + secs(1));
    add(&f.left("newleft.txt"), t0() + secs(300));
    add(&f.right("newleft.txt"), t0());
    add(&f.left("newright.txt"), t0());
    add(&f.right("newright.txt"), t0() + secs(300));
    add(&f.left("onlyleft.txt"), t0());
    add(&f.right("onlyright.txt"), t0());

    let r = f.compare();

    assert_eq!(DiffStatus::Identical, find(&r, "same.txt").status);
    assert_eq!(DiffStatus::NewerLeft, find(&r, "newleft.txt").status);
    assert_eq!(DiffStatus::NewerRight, find(&r, "newright.txt").status);
    assert_eq!(DiffStatus::OnlyLeft, find(&r, "onlyleft.txt").status);
    assert_eq!(DiffStatus::OnlyRight, find(&r, "onlyright.txt").status);
    assert!(find(&r, "onlyleft.txt").right.is_none());
    assert!(find(&r, "onlyright.txt").left.is_none());
}

#[test]
fn same_time_different_size_is_detected_without_content_compare() {
    let f = Fixture::new();
    add_file(&f.left(r"d\a.txt"), t0(), b"short");
    add_file(&f.right(r"d\a.txt"), t0(), b"a much longer content");
    let r = f.compare();
    assert_eq!(
        DiffStatus::SameTimeDifferentContent,
        find(&r, r"d\a.txt").status
    );
    assert_eq!(DiffStatus::Differs, find(&r, "d").status);
    assert_eq!(DiffStatus::Differs, r.tree.root().status);
}

#[test]
fn different_time_different_size_keeps_direction() {
    let f = Fixture::new();
    add_file(&f.left("a.txt"), t0() + secs(300), b"short");
    add_file(&f.right("a.txt"), t0(), b"a much longer content");
    assert_eq!(DiffStatus::NewerLeft, find(&f.compare(), "a.txt").status);
}

#[test]
fn content_compare_same_time_same_size_different_content() {
    let f = Fixture::new();
    add_file(&f.left(r"d\a.txt"), t0(), b"abc");
    add_file(&f.right(r"d\a.txt"), t0(), b"xyz");
    add_file(&f.left(r"d\b.txt"), t0(), b"same");
    add_file(&f.right(r"d\b.txt"), t0(), b"same");

    assert_eq!(DiffStatus::Identical, find(&f.compare(), r"d\a.txt").status);

    let r = f.compare_with(&content(ContentCompareMode::WhenTimestampsEqual));
    assert_eq!(
        DiffStatus::SameTimeDifferentContent,
        find(&r, r"d\a.txt").status
    );
    assert_eq!(DiffStatus::Identical, find(&r, r"d\b.txt").status);
    assert_eq!(DiffStatus::Differs, find(&r, "d").status);
    assert_eq!(DiffStatus::Differs, r.tree.root().status);
}

#[test]
fn content_compare_time_only_differences_roll_up_as_identical() {
    let f = Fixture::new();
    add_file(&f.left(r"d\a.txt"), t0() + secs(86400), b"abc");
    add_file(&f.right(r"d\a.txt"), t0(), b"abc");
    add_file(&f.left(r"d\b.txt"), t0(), b"same");
    add_file(&f.right(r"d\b.txt"), t0(), b"same");

    let r = f.compare_with(&content(ContentCompareMode::WhenSizesEqual));

    assert_eq!(
        DiffStatus::SameContentDifferentTime,
        find(&r, r"d\a.txt").status
    );
    assert_eq!(DiffStatus::Identical, find(&r, "d").status);
    assert_eq!(DiffStatus::Identical, r.tree.root().status);
}

#[cfg(windows)]
#[test]
fn content_compare_locked_file_is_error_with_message() {
    use std::os::windows::fs::OpenOptionsExt;
    let f = Fixture::new();
    add_file(&f.left("a.txt"), t0(), b"abc");
    add_file(&f.right("a.txt"), t0(), b"abc");
    let _lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .share_mode(0)
        .open(f.right("a.txt"))
        .unwrap();

    let r = f.compare_with(&content(ContentCompareMode::WhenTimestampsEqual));
    let node = find(&r, "a.txt");

    assert_eq!(DiffStatus::Error, node.status);
    assert!(
        node.error
            .as_deref()
            .unwrap()
            .starts_with("Content compare failed:")
    );
}

fn collect(f: &Fixture, options: &CompareOptions) -> Vec<ScanProgress> {
    let reports = Mutex::new(Vec::new());
    let report = |p: ScanProgress| reports.lock().unwrap().push(p);
    DiffEngine::new()
        .compare(
            &f.ls(),
            &f.rs(),
            options,
            Some(&report),
            &AtomicBool::new(false),
        )
        .unwrap();
    reports.into_inner().unwrap()
}

#[test]
fn content_compare_reports_bytes() {
    let f = Fixture::new();
    add_file(&f.left("a.txt"), t0(), b"abcd");
    add_file(&f.right("a.txt"), t0(), b"abcd");
    add_file(&f.left("b.txt"), t0(), b"123456");
    add_file(&f.right("b.txt"), t0(), b"654321");

    let reports = collect(&f, &content(ContentCompareMode::WhenTimestampsEqual));

    assert!(reports.iter().any(|p| p.bytes_total == 10));
    assert_eq!(4, reports.last().unwrap().items_scanned);
}

#[test]
fn refresh_applies_content_compare() {
    let f = Fixture::new();
    add_file(&f.left(r"d\a.txt"), t0(), b"abc");
    add_file(&f.right(r"d\a.txt"), t0(), b"abc");
    let mut r = f.compare_with(&content(ContentCompareMode::WhenTimestampsEqual));
    assert_eq!(DiffStatus::Identical, r.tree.root().status);

    add_file(&f.right(r"d\a.txt"), t0(), b"xyz");
    let node = r.tree.node(find_id(&r, r"d\a.txt")).clone();
    DiffEngine::new()
        .refresh(&mut r, node.parent.unwrap(), &node.name)
        .unwrap();

    assert_eq!(
        DiffStatus::SameTimeDifferentContent,
        find(&r, r"d\a.txt").status
    );
    assert_eq!(DiffStatus::Differs, find(&r, "d").status);
    assert_eq!(DiffStatus::Differs, r.tree.root().status);
}

#[test]
fn tolerance_edge_cases() {
    for (millis, expected) in [
        (2000, DiffStatus::Identical),
        (1999, DiffStatus::Identical),
        (2001, DiffStatus::NewerLeft),
    ] {
        let f = Fixture::new();
        add(&f.left("a.txt"), t0() + ms(millis));
        add(&f.right("a.txt"), t0());
        assert_eq!(expected, find(&f.compare(), "a.txt").status, "{millis} ms");
    }
}

#[test]
fn dst_option() {
    let f = Fixture::new();
    add(&f.left("a.txt"), t0() + secs(3600));
    add(&f.right("a.txt"), t0());
    assert_eq!(DiffStatus::NewerLeft, find(&f.compare(), "a.txt").status);
    let options = CompareOptions {
        ignore_dst_offset: true,
        ..CompareOptions::default()
    };
    assert_eq!(
        DiffStatus::Identical,
        find(&f.compare_with(&options), "a.txt").status
    );
}

#[test]
fn path_matching_ignore_case() {
    let f = Fixture::new();
    add(&f.left(r"Docs\Readme.TXT"), t0());
    add(&f.right(r"DOCS\readme.txt"), t0());
    let r = f.compare_with(&CompareOptions {
        path_comparison: Some(PathComparison::IgnoreCase),
        ..CompareOptions::default()
    });
    let top = root_children(&r);
    assert_eq!(1, top.len());
    assert_eq!(1, top[0].children.len());
    let file = r.tree.node(top[0].children[0]);
    assert_eq!(DiffStatus::Identical, file.status);
    assert!(file.left.is_some() && file.right.is_some());
}

#[test]
fn path_matching_case_sensitive_keeps_names_apart() {
    let f = Fixture::new();
    add(&f.left(r"Docs\Readme.TXT"), t0());
    add(&f.right(r"DOCS\readme.txt"), t0());
    let r = f.compare_with(&CompareOptions {
        path_comparison: Some(PathComparison::CaseSensitive),
        ..CompareOptions::default()
    });
    let top = root_children(&r);
    assert_eq!(2, top.len());
    assert_eq!(
        DiffStatus::OnlyLeft,
        top.iter().find(|c| c.name == "Docs").unwrap().status
    );
    assert_eq!(
        DiffStatus::OnlyRight,
        top.iter().find(|c| c.name == "DOCS").unwrap().status
    );
}

#[test]
fn path_comparison_default_follows_the_os() {
    let f = Fixture::new();
    let r = f.compare();
    assert_eq!(
        !cfg!(target_os = "linux"),
        PathComparison::for_current_os().is_ignore_case()
    );
    // Temp folders are on the default (case-insensitive on Windows and macOS) file system.
    assert_eq!(!cfg!(target_os = "linux"), r.paths().is_ignore_case());
}

#[cfg(target_os = "linux")]
#[test]
fn case_only_names_in_one_folder_are_two_rows_on_linux() {
    let f = Fixture::new();
    add_file(&f.left("a.txt"), t0(), b"lower");
    add_file(&f.left("A.txt"), t0(), b"upper");
    add_file(&f.right("a.txt"), t0(), b"lower");
    let r = f.compare();
    let top = root_children(&r);
    assert_eq!(2, top.len());
    assert_eq!(
        DiffStatus::Identical,
        top.iter().find(|c| c.name == "a.txt").unwrap().status
    );
    assert_eq!(
        DiffStatus::OnlyLeft,
        top.iter().find(|c| c.name == "A.txt").unwrap().status
    );
}

#[cfg(target_os = "linux")]
#[test]
fn case_only_folder_names_are_separate_on_linux() {
    let f = Fixture::new();
    add(&f.left(r"Docs\a.txt"), t0());
    add(&f.right(r"docs\a.txt"), t0());
    let r = f.compare();
    assert_eq!(2, root_children(&r).len());
    assert_eq!(DiffStatus::OnlyLeft, find(&r, "Docs").status);
    assert_eq!(DiffStatus::OnlyRight, find(&r, "docs").status);
}

#[test]
fn type_conflict_file_vs_folder() {
    let f = Fixture::new();
    add(&f.left("thing"), t0());
    add(&f.right(r"thing\inner.txt"), t0());
    let r = f.compare();
    let node = find(&r, "thing");
    assert_eq!(DiffStatus::TypeConflict, node.status);
    assert_eq!(
        Some("File on the left, folder on the right."),
        node.error.as_deref()
    );
    assert_eq!(DiffStatus::Differs, r.tree.root().status);
}

#[test]
fn folder_roll_up() {
    let f = Fixture::new();
    add(&f.left(r"same\a.txt"), t0());
    add(&f.right(r"same\a.txt"), t0());
    add(&f.left(r"same\deep\b.txt"), t0());
    add(&f.right(r"same\deep\b.txt"), t0());
    add(&f.left(r"diff\a.txt"), t0());
    add(&f.right(r"diff\a.txt"), t0());
    add(&f.left(r"diff\deep\b.txt"), t0() + secs(3 * 3600));
    add(&f.right(r"diff\deep\b.txt"), t0());
    add(&f.left(r"onlyl\x.txt"), t0());
    fs::create_dir_all(f.left("empty")).unwrap();
    fs::create_dir_all(f.right("empty")).unwrap();

    let r = f.compare();
    assert_eq!(DiffStatus::Identical, find(&r, "same").status);
    assert_eq!(DiffStatus::Identical, find(&r, r"same\deep").status);
    assert_eq!(DiffStatus::Differs, find(&r, "diff").status);
    assert_eq!(DiffStatus::Differs, find(&r, r"diff\deep").status);
    assert_eq!(DiffStatus::OnlyLeft, find(&r, "onlyl").status);
    assert_eq!(DiffStatus::OnlyLeft, find(&r, r"onlyl\x.txt").status);
    assert_eq!(DiffStatus::Identical, find(&r, "empty").status);
    assert_eq!(DiffStatus::Differs, r.tree.root().status);
}

#[test]
fn folder_sizes_are_summed() {
    let f = Fixture::new();
    add_file(&f.left(r"d\a.txt"), t0(), b"12345");
    add_file(&f.left(r"d\e\b.txt"), t0(), b"123");
    let r = f.compare();
    let d = find(&r, "d");
    assert_eq!(8, d.left_total);
    assert_eq!(0, d.right_total);
}

#[test]
fn exclude_patterns_skip_files_and_folders() {
    let f = Fixture::new();
    add(&f.left("keep.txt"), t0());
    add(&f.left("junk.tmp"), t0());
    add(&f.left(r"node_modules\pkg\index.js"), t0());
    add(&f.right(r"obj\x.dll"), t0());
    let r = f.compare_with(&CompareOptions {
        exclude_patterns: "*.tmp;node_modules;obj".into(),
        ..CompareOptions::default()
    });
    assert!(try_find(&r, "keep.txt").is_some());
    assert!(try_find(&r, "junk.tmp").is_none());
    assert!(try_find(&r, "node_modules").is_none());
    assert!(try_find(&r, "obj").is_none());
}

#[test]
fn include_patterns_restrict_files_but_keep_folders() {
    let f = Fixture::new();
    add(&f.left(r"src\a.cs"), t0());
    add(&f.left(r"src\b.txt"), t0());
    let r = f.compare_with(&CompareOptions {
        include_patterns: "*.cs".into(),
        ..CompareOptions::default()
    });
    assert!(try_find(&r, r"src\a.cs").is_some());
    assert!(try_find(&r, r"src\b.txt").is_none());
}

#[test]
fn ignore_hidden_and_system() {
    let f = Fixture::new();
    let hidden = if cfg!(windows) {
        "hidden.txt"
    } else {
        ".hidden.txt"
    };
    add(&f.left("visible.txt"), t0());
    add(&f.left(hidden), t0());
    make_hidden(&f.left(hidden));
    assert!(try_find(&f.compare(), hidden).is_some());
    let r = f.compare_with(&CompareOptions {
        ignore_hidden_and_system: true,
        ..CompareOptions::default()
    });
    assert!(try_find(&r, hidden).is_none());
    assert!(try_find(&r, "visible.txt").is_some());
}

#[test]
fn non_recursive_only_top_level() {
    let f = Fixture::new();
    add(&f.left("top.txt"), t0());
    add(&f.left(r"sub\deep.txt"), t0());
    let r = f.compare_with(&CompareOptions {
        recursive: false,
        ..CompareOptions::default()
    });
    assert!(try_find(&r, "top.txt").is_some());
    assert!(find(&r, "sub").children.is_empty());
}

#[test]
fn symlink_loop_is_detected_when_following() {
    let f = Fixture::new();
    add(&f.left(r"a\file.txt"), t0());
    if !make_dir_link(&f.left(r"a\loop"), &f.left("a")) {
        eprintln!("skipped: can't create folder links here");
        return;
    }

    let r = f.compare_with(&CompareOptions {
        symlink_mode: SymlinkMode::Follow,
        ..CompareOptions::default()
    });
    let lp = find(&r, r"a\loop");
    assert_eq!(DiffStatus::Error, lp.status);
    assert!(lp.error.as_deref().unwrap().to_lowercase().contains("loop"));
    assert!(lp.children.is_empty());
}

#[test]
fn symlinks_followed_show_target_contents() {
    let f = Fixture::new();
    add(&f.left(r"target\file.txt"), t0());
    if !make_dir_link(&f.left("link"), &f.left("target")) {
        return;
    }
    let r = f.compare_with(&CompareOptions {
        symlink_mode: SymlinkMode::Follow,
        ..CompareOptions::default()
    });
    assert_eq!(DiffStatus::OnlyLeft, find(&r, r"link\file.txt").status);
}

#[test]
fn symlinks_skipped_by_default() {
    let f = Fixture::new();
    add(&f.left(r"target\file.txt"), t0());
    if !make_dir_link(&f.left("link"), &f.left("target")) {
        return;
    }
    let r = f.compare();
    assert!(try_find(&r, "link").is_none());
    assert!(try_find(&r, r"target\file.txt").is_some());
}

#[test]
fn scan_does_not_modify_file_system() {
    let f = Fixture::new();
    add(&f.left(r"a\b.txt"), t0());
    add(&f.right("c.txt"), t0());
    let before = snapshot_tree(f.dir.path());
    f.compare_with(&content(ContentCompareMode::WhenSizesEqual));
    assert_eq!(before, snapshot_tree(f.dir.path()));
}

#[test]
fn scan_can_be_cancelled() {
    let f = Fixture::new();
    for i in 0..50 {
        add(&f.left(&format!(r"d{i}\f.txt")), t0());
    }
    let result = DiffEngine::new().compare(
        &f.ls(),
        &f.rs(),
        &CompareOptions::default(),
        None,
        &AtomicBool::new(true),
    );
    assert_eq!(Some(CompareError::Cancelled), result.err());
}

#[test]
fn scan_reports_progress() {
    let f = Fixture::new();
    for i in 0..10 {
        add(&f.left(&format!("f{i}.txt")), t0());
    }
    let reports = collect(&f, &CompareOptions::default());
    assert_eq!(10, reports.last().unwrap().items_scanned);
}

#[test]
fn rejects_same_or_nested_roots() {
    let f = Fixture::new();
    fs::create_dir_all(f.left("inner")).unwrap();
    let engine = DiffEngine::new();
    let o = CompareOptions::default();
    let no = AtomicBool::new(false);
    let sep = std::path::MAIN_SEPARATOR;
    assert_eq!(
        Some(CompareError::InvalidRoots(
            "Left and right are the same folder.".into()
        )),
        engine
            .compare(&f.ls(), &format!("{}{sep}", f.ls()), &o, None, &no)
            .err()
    );
    assert_eq!(
        Some(CompareError::InvalidRoots(
            "One folder is inside the other. Choose two independent folders.".into()
        )),
        engine
            .compare(&f.ls(), &f.left("inner").to_string_lossy(), &o, None, &no)
            .err()
    );
}

#[test]
fn refresh_updates_node_and_ancestors() {
    let f = Fixture::new();
    add(&f.left(r"d\a.txt"), t0() + secs(3600));
    add(&f.right(r"d\a.txt"), t0());
    let mut r = f.compare();
    let id = find_id(&r, r"d\a.txt");
    assert_eq!(DiffStatus::Differs, r.tree.root().status);

    set_mtime(&f.right(r"d\a.txt"), t0() + secs(3600));
    let (parent, name) = {
        let n = r.tree.node(id);
        (n.parent.unwrap(), n.name.clone())
    };
    DiffEngine::new().refresh(&mut r, parent, &name).unwrap();

    assert_eq!(DiffStatus::Identical, find(&r, r"d\a.txt").status);
    assert_eq!(DiffStatus::Identical, find(&r, "d").status);
    assert_eq!(DiffStatus::Identical, r.tree.root().status);
    assert!(!r.tree.node(id).alive);
}

#[test]
fn refresh_removes_deleted_node() {
    let f = Fixture::new();
    add(&f.left("a.txt"), t0());
    let mut r = f.compare();
    fs::remove_file(f.left("a.txt")).unwrap();
    DiffEngine::new()
        .refresh(&mut r, NodeId::ROOT, "a.txt")
        .unwrap();
    assert!(r.tree.root().children.is_empty());
    assert_eq!(DiffStatus::Identical, r.tree.root().status);
}

#[test]
fn refresh_adds_new_node() {
    let f = Fixture::new();
    add(&f.left("a.txt"), t0());
    let mut r = f.compare();
    add(&f.right(r"new\b.txt"), t0());
    DiffEngine::new()
        .refresh(&mut r, NodeId::ROOT, "new")
        .unwrap();
    assert_eq!(DiffStatus::OnlyRight, find(&r, r"new\b.txt").status);
    assert_eq!(4, find(&r, "new").right_total);
    assert_eq!(DiffStatus::Differs, r.tree.root().status);
}

#[test]
fn unreadable_root_entry_is_error_not_abort() {
    // A file where a folder is expected on one side is a type conflict, never an aborted scan.
    let f = Fixture::new();
    add(&f.left(r"x\a.txt"), t0());
    add(&f.right("x"), t0());
    add(&f.left("ok.txt"), t0());
    add(&f.right("ok.txt"), t0());
    let r = f.compare();
    assert_eq!(
        Some("Folder on the left, file on the right."),
        find(&r, "x").error.as_deref()
    );
    assert_eq!(DiffStatus::Identical, find(&r, "ok.txt").status);
}

#[test]
fn right_only_entries_follow_left_order() {
    let f = Fixture::new();
    add(&f.left("b.txt"), t0());
    add(&f.left("a.txt"), t0());
    add(&f.right("c.txt"), t0());
    add(&f.right("a.txt"), t0());
    let r = f.compare();
    let names: Vec<_> = root_children(&r).iter().map(|n| n.name.clone()).collect();
    assert_eq!("c.txt", names.last().unwrap());
    assert_eq!(3, names.len());
}
