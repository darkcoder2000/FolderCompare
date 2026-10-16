mod common;

use std::fs;
use std::path::{MAIN_SEPARATOR, Path};

use common::*;
use fc_core::ops::{ensure_inside_root, validate_roots};

#[test]
fn valid_roots() {
    let f = Fixture::new();
    assert_eq!(None, validate_roots(&f.ls(), &f.rs()));
}

#[test]
fn same_root_is_rejected() {
    let f = Fixture::new();
    let same = Some("Left and right are the same folder.".to_string());
    assert_eq!(same, validate_roots(&f.ls(), &f.ls()));
    assert_eq!(
        same,
        validate_roots(&f.ls(), &format!("{}{MAIN_SEPARATOR}", f.ls()))
    );
    assert_eq!(
        same,
        validate_roots(&f.ls(), &f.r.join("..").join("left").to_string_lossy())
    );
    if !cfg!(target_os = "linux") {
        assert_eq!(same, validate_roots(&f.ls(), &f.ls().to_uppercase()));
    }
}

#[test]
fn nested_root_is_rejected() {
    let f = Fixture::new();
    fs::create_dir_all(f.left("sub")).unwrap();
    let nested =
        Some("One folder is inside the other. Choose two independent folders.".to_string());
    let sub = f.left("sub").to_string_lossy().into_owned();
    assert_eq!(nested, validate_roots(&f.ls(), &sub));
    assert_eq!(nested, validate_roots(&sub, &f.ls()));
}

#[test]
fn similar_prefix_is_not_nested() {
    let f = Fixture::new();
    let left2 = f.dir.path().join("left2");
    fs::create_dir_all(&left2).unwrap();
    assert_eq!(None, validate_roots(&f.ls(), &left2.to_string_lossy()));
}

#[test]
fn missing_root_is_rejected() {
    let f = Fixture::new();
    let nope = f.dir.path().join("nope");
    assert_eq!(
        Some(format!(
            "Right folder does not exist or is not accessible: {}",
            nope.display()
        )),
        validate_roots(&f.ls(), &nope.to_string_lossy())
    );
    assert!(
        validate_roots(&nope.to_string_lossy(), &f.rs())
            .unwrap()
            .starts_with("Left folder does not exist")
    );
}

#[test]
fn empty_root_is_rejected() {
    let f = Fixture::new();
    assert_eq!(
        Some("Please choose both folders.".to_string()),
        validate_roots(&f.ls(), " ")
    );
}

#[test]
fn invalid_path_is_rejected() {
    let f = Fixture::new();
    assert!(
        validate_roots(&f.ls(), "a\0b")
            .unwrap()
            .starts_with("Invalid path: ")
    );
}

#[test]
fn ensure_inside_root_does_not_ignore_case() {
    // A differently cased root may be another folder on a case-sensitive file system, so it is never accepted.
    let f = Fixture::new();
    let upper = Path::new(&f.ls().to_uppercase()).join("a.txt");
    assert!(ensure_inside_root(&f.l, &upper).is_err());
}

#[test]
fn validate_roots_ignores_case_for_nesting() {
    let f = Fixture::new();
    fs::create_dir_all(f.left("Sub")).unwrap();
    if !cfg!(target_os = "linux") {
        let upper = Path::new(&f.ls().to_uppercase()).join("Sub");
        assert!(validate_roots(&upper.to_string_lossy(), &f.ls()).is_some());
    }
    assert!(validate_roots(&f.left("Sub").to_string_lossy(), &f.ls()).is_some());
}

#[test]
fn ensure_inside_root_cases() {
    let f = Fixture::new();
    let cases = [
        (r"left\a.txt", true),
        (r"left\sub\..\a.txt", true),
        (r"left\..\right\a.txt", false),
        (r"left\..\left2\a.txt", false),
        ("left", false),
        (r"left\", false),
        (r"other\a.txt", false),
    ];
    for (rel, allowed) in cases {
        let path = f.dir.path().join(p(rel));
        let result = ensure_inside_root(&f.l, &path);
        assert_eq!(allowed, result.is_ok(), "{rel}: {result:?}");
        if let Err(e) = result {
            assert!(e.0.starts_with("Refusing to operate on '"), "{}", e.0);
            assert!(e.0.ends_with(&format!("because it is outside the root '{}'.", f.ls())));
        }
    }
    assert_eq!(
        f.left("a.txt"),
        fc_core::ops::ensure_inside_root(&f.l, &f.dir.path().join(p(r"left\sub\..\a.txt")))
            .unwrap()
    );
}
