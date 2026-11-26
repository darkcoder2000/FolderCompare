//! Registry layout of the Windows Explorer context menu. The entries are `IExplorerCommand` handlers in
//! FolderCompare.ShellExt.dll, which decides visibility and titles itself and keeps the selected left item.
//! The CLSIDs and the `Software\FolderCompare` value names must match src/FolderCompare.ShellExt/ShellExt.cpp.
//! Pure data, so it is testable on every OS.

use std::path::Path;

/// One registry value below HKCU. An empty `name` is the key's default value.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RegistryValue {
    pub sub_key: String,
    pub name: String,
    pub value: String,
}

impl RegistryValue {
    pub fn new(
        sub_key: impl Into<String>,
        name: impl Into<String>,
        value: impl Into<String>,
    ) -> Self {
        Self {
            sub_key: sub_key.into(),
            name: name.into(),
            value: value.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairKind {
    BothFolders,
    BothFiles,
    Mixed,
    Missing,
}

pub const CLASSES_ROOT: &str = r"Software\Classes";
/// Holds [`EXE_PATH_VALUE`], read by the DLL to start the comparison.
pub const APP_KEY: &str = r"Software\FolderCompare";
pub const EXE_PATH_VALUE: &str = "ExePath";
/// The item picked with "Select as left" (values Path, IsFolder). Written and cleared by the DLL.
pub const LEFT_SELECTION_KEY: &str = r"Software\FolderCompare\LeftSelection";

pub const COMPARE_VERB: &str = "FolderCompare.Compare";
pub const SELECT_LEFT_VERB: &str = "FolderCompare.SelectLeft";
/// Verb of version 1.2.0 (static "Compare to left" entry), removed when registering.
pub const LEGACY_COMPARE_TO_LEFT_VERB: &str = "FolderCompare.CompareToLeft";

pub const COMPARE_CLSID: &str = "{501F238C-E07D-479E-9123-448B4646A913}";
pub const SELECT_LEFT_CLSID: &str = "{0E804BB4-FBFB-4028-B0C3-E348EF8E0F71}";
pub const CLSIDS: [&str; 2] = [COMPARE_CLSID, SELECT_LEFT_CLSID];

/// Files and folders.
pub const CLASS_KEYS: [&str; 2] = ["*", "Directory"];

pub fn verb_key(class_key: &str, verb: &str) -> String {
    format!(r"{class_key}\shell\{verb}")
}

/// All verb keys below [`CLASSES_ROOT`] to delete before registering and when unregistering.
pub fn verb_keys() -> Vec<String> {
    CLASS_KEYS
        .iter()
        .flat_map(|c| {
            [COMPARE_VERB, SELECT_LEFT_VERB, LEGACY_COMPARE_TO_LEFT_VERB].map(|v| verb_key(c, v))
        })
        .collect()
}

/// Full HKCU subkeys that only exist in the 1.2.0 layout. Any of them present means an old exe registered its
/// entries again and the layout must be rewritten.
pub fn legacy_keys() -> Vec<String> {
    CLASS_KEYS
        .iter()
        .flat_map(|c| {
            [
                format!(
                    r"{CLASSES_ROOT}\{}",
                    verb_key(c, LEGACY_COMPARE_TO_LEFT_VERB)
                ),
                format!(r"{CLASSES_ROOT}\{}\command", verb_key(c, SELECT_LEFT_VERB)),
            ]
        })
        .collect()
}

pub fn clsid_key(clsid: &str) -> String {
    format!(r"CLSID\{clsid}")
}

pub fn inproc_server_key(clsid: &str) -> String {
    format!(r"{CLASSES_ROOT}\{}\InprocServer32", clsid_key(clsid))
}

/// File name of the extracted DLL. The content hash keeps a DLL that Explorer still has loaded from being
/// overwritten.
pub fn dll_file_name(hash: &str) -> String {
    format!("FolderCompare.ShellExt.{hash}.dll")
}

/// The values to write, as full subkeys of HKCU.
pub fn build(exe_path: &str, dll_path: &str) -> Vec<RegistryValue> {
    let icon = format!("\"{exe_path}\",0");
    let mut values = Vec::new();
    for (clsid, name) in [
        (COMPARE_CLSID, "FolderCompare Compare"),
        (SELECT_LEFT_CLSID, "FolderCompare Select Left"),
    ] {
        values.push(RegistryValue::new(
            format!(r"{CLASSES_ROOT}\{}", clsid_key(clsid)),
            "",
            name,
        ));
        values.push(RegistryValue::new(inproc_server_key(clsid), "", dll_path));
        values.push(RegistryValue::new(
            inproc_server_key(clsid),
            "ThreadingModel",
            "Apartment",
        ));
    }
    for class_key in CLASS_KEYS {
        for (verb, clsid) in [
            (COMPARE_VERB, COMPARE_CLSID),
            (SELECT_LEFT_VERB, SELECT_LEFT_CLSID),
        ] {
            let key = format!(r"{CLASSES_ROOT}\{}", verb_key(class_key, verb));
            values.push(RegistryValue::new(&key, "ExplorerCommandHandler", clsid));
            // All selected items in one call.
            values.push(RegistryValue::new(&key, "MultiSelectModel", "Player"));
            values.push(RegistryValue::new(&key, "Icon", &icon));
        }
    }
    values.push(RegistryValue::new(APP_KEY, EXE_PATH_VALUE, exe_path));
    values
}

pub fn classify(left: &Path, right: &Path) -> PairKind {
    let (left_dir, right_dir) = (left.is_dir(), right.is_dir());
    let (left_file, right_file) = (!left_dir && left.is_file(), !right_dir && right.is_file());
    if !(left_dir || left_file) || !(right_dir || right_file) {
        PairKind::Missing
    } else if left_dir && right_dir {
        PairKind::BothFolders
    } else if left_file && right_file {
        PairKind::BothFiles
    } else {
        PairKind::Mixed
    }
}
