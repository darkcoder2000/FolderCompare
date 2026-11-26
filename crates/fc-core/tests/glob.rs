mod common;

use common::p;
use fc_core::compare::PathPatternMatcher;

#[test]
fn exclude() {
    let cases = [
        ("*.tmp", "file.tmp", r"a\file.tmp", true),
        ("*.tmp", "file.TMP", "file.TMP", true),
        ("*.tmp", "file.tmpx", "file.tmpx", false),
        ("node_modules", "node_modules", r"src\node_modules", true),
        ("node_modules", "node_modules2", "node_modules2", false),
        ("bin;obj", "obj", r"x\obj", true),
        (" bin ; obj ", "bin", "bin", true),
        ("file?.txt", "file1.txt", "file1.txt", true),
        ("file?.txt", "file12.txt", "file12.txt", false),
        (r"src\gen", "gen", r"src\gen", true),
        (r"src\gen", "gen", r"other\gen", false),
        ("src/*/tmp", "tmp", r"src\a\tmp", true),
        (r"**\cache", "cache", r"a\b\cache", true),
        ("a+b(1).txt", "a+b(1).txt", "a+b(1).txt", true),
        ("", "anything", "anything", false),
    ];
    for (patterns, name, rel, excluded) in cases {
        assert_eq!(
            excluded,
            PathPatternMatcher::new("", patterns).is_excluded(name, &p(rel)),
            "{patterns} / {rel}"
        );
    }
}

#[test]
fn double_star_needs_a_parent_folder() {
    let m = PathPatternMatcher::new("", r"**\cache");
    assert!(!m.is_excluded("cache", "cache"));
}

#[test]
fn star_does_not_cross_separators() {
    let m = PathPatternMatcher::new("", "src/*.cs");
    assert!(m.is_excluded("a.cs", &p(r"src\a.cs")));
    assert!(!m.is_excluded("a.cs", &p(r"src\sub\a.cs")));
}

#[test]
fn empty_include_includes_everything() {
    assert!(PathPatternMatcher::new("", "").is_included_file("x.bin", "x.bin"));
}

#[test]
fn include_restricts_files() {
    let m = PathPatternMatcher::new("*.cs;*.xaml", "");
    assert!(m.is_included_file("A.CS", "A.CS"));
    assert!(m.is_included_file("Main.xaml", &p(r"v\Main.xaml")));
    assert!(!m.is_included_file("readme.md", "readme.md"));
}
