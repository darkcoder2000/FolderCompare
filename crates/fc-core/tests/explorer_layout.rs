mod common;

use common::*;
use fc_core::shell_layout::*;

const EXE: &str = r"C:\Program Files\Tools\FolderCompare.exe";
const DLL: &str =
    r"C:\Users\me\AppData\Local\FolderCompare\ShellExt\FolderCompare.ShellExt.0123abcd.dll";

fn v(key: &str, name: &str, value: &str) -> RegistryValue {
    RegistryValue::new(key, name, value)
}

#[test]
fn build_registers_both_handlers_per_user() {
    let values = build(EXE, DLL);
    for clsid in [COMPARE_CLSID, SELECT_LEFT_CLSID] {
        let key = format!(r"Software\Classes\CLSID\{clsid}\InprocServer32");
        assert!(values.contains(&v(&key, "", DLL)));
        assert!(values.contains(&v(&key, "ThreadingModel", "Apartment")));
    }
    assert!(values.contains(&v(r"Software\FolderCompare", "ExePath", EXE)));
    assert!(values.iter().all(|x| x.sub_key.starts_with(r"Software\")));
}

#[test]
fn build_verbs_use_explorer_command_handlers() {
    let values = build(EXE, DLL);
    for class_key in ["*", "Directory"] {
        let compare = format!(r"Software\Classes\{class_key}\shell\FolderCompare.Compare");
        let select = format!(r"Software\Classes\{class_key}\shell\FolderCompare.SelectLeft");
        assert!(values.contains(&v(&compare, "ExplorerCommandHandler", COMPARE_CLSID)));
        assert!(values.contains(&v(&select, "ExplorerCommandHandler", SELECT_LEFT_CLSID)));
        assert!(values.contains(&v(&compare, "MultiSelectModel", "Player")));
        assert!(values.contains(&v(&select, "MultiSelectModel", "Player")));
        assert!(values.contains(&v(&compare, "Icon", &format!("\"{EXE}\",0"))));
    }
}

#[test]
fn build_has_no_static_labels_or_commands() {
    let values = build(EXE, DLL);
    assert!(!values.iter().any(|x| x.name == "MUIVerb"));
    assert!(!values.iter().any(|x| x.sub_key.ends_with(r"\command")));
    assert!(
        !values
            .iter()
            .any(|x| x.value.to_lowercase().contains("compare to left"))
    );
    assert_eq!(2 * 3 + 2 * 2 * 3 + 1, values.len());
}

#[test]
fn compare_verb_sorts_before_select_left() {
    assert!(COMPARE_VERB < SELECT_LEFT_VERB);
}

#[test]
fn verb_keys_include_legacy_compare_to_left() {
    assert_eq!(
        vec![
            r"*\shell\FolderCompare.Compare",
            r"*\shell\FolderCompare.SelectLeft",
            r"*\shell\FolderCompare.CompareToLeft",
            r"Directory\shell\FolderCompare.Compare",
            r"Directory\shell\FolderCompare.SelectLeft",
            r"Directory\shell\FolderCompare.CompareToLeft",
        ],
        verb_keys()
    );
}

#[test]
fn legacy_keys_are_never_written_by_build() {
    assert_eq!(
        vec![
            r"Software\Classes\*\shell\FolderCompare.CompareToLeft",
            r"Software\Classes\*\shell\FolderCompare.SelectLeft\command",
            r"Software\Classes\Directory\shell\FolderCompare.CompareToLeft",
            r"Software\Classes\Directory\shell\FolderCompare.SelectLeft\command",
        ],
        legacy_keys()
    );
    let written: Vec<String> = build(EXE, DLL).into_iter().map(|x| x.sub_key).collect();
    assert!(legacy_keys().iter().all(|k| !written.contains(k)));
}

#[test]
fn dll_file_name_contains_hash() {
    assert_eq!(
        "FolderCompare.ShellExt.0123abcd.dll",
        dll_file_name("0123abcd")
    );
}

#[test]
fn classify_pairs() {
    let f = Fixture::new();
    add(&f.left("a.txt"), t0());
    add(&f.right("b.txt"), t0());
    let gone = f.dir.path().join("gone");
    assert_eq!(PairKind::BothFolders, classify(&f.l, &f.r));
    assert_eq!(
        PairKind::BothFiles,
        classify(&f.left("a.txt"), &f.right("b.txt"))
    );
    assert_eq!(PairKind::Mixed, classify(&f.l, &f.right("b.txt")));
    assert_eq!(PairKind::Missing, classify(&gone, &f.r));
    assert_eq!(PairKind::Missing, classify(&f.left("a.txt"), &gone));
}
