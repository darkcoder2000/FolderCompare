//! The "Compare" / "Select as left" entries in the Windows Explorer context menu (per user, HKCU). They are handled
//! by FolderCompare.ShellExt.dll, which is embedded in the exe and extracted to `%LOCALAPPDATA%`. Other operating
//! systems (and builds without the DLL) report the integration as unsupported and the menu entry is hidden.

/// The integration exists in this build (Windows x64 with the embedded DLL).
pub fn is_supported() -> bool {
    cfg!(all(windows, shellext))
}

#[cfg(all(windows, shellext))]
pub use imp::{ensure_current_path, is_registered, register, unregister};

#[cfg(not(all(windows, shellext)))]
pub fn is_registered() -> bool {
    false
}

#[cfg(not(all(windows, shellext)))]
pub fn register() -> Result<(), String> {
    Err("The Explorer context menu integration is not available in this build.".into())
}

#[cfg(not(all(windows, shellext)))]
pub fn unregister() -> Result<(), String> {
    Ok(())
}

#[cfg(not(all(windows, shellext)))]
pub fn ensure_current_path() {}

#[cfg(all(windows, shellext))]
mod imp {
    use std::fs;
    use std::io;
    use std::path::PathBuf;

    use fc_core::shell_layout as layout;
    use sha2::{Digest, Sha256};
    use tracing::{info, warn};
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;

    static DLL: &[u8] = include_bytes!(env!("FC_SHELLEXT_DLL"));

    fn dll_dir() -> PathBuf {
        dirs::data_local_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("FolderCompare")
            .join("ShellExt")
    }

    fn exe_path() -> Result<String, String> {
        std::env::current_exe()
            .map(|p| p.display().to_string())
            .map_err(|e| e.to_string())
    }

    fn expected_dll_path() -> PathBuf {
        let hash = Sha256::digest(DLL);
        let hex: String = hash.iter().take(4).map(|b| format!("{b:02x}")).collect();
        dll_dir().join(layout::dll_file_name(&hex))
    }

    fn hkcu() -> RegKey {
        RegKey::predef(HKEY_CURRENT_USER)
    }

    fn probe_key() -> String {
        format!(
            r"{}\{}",
            layout::CLASSES_ROOT,
            layout::verb_key("Directory", layout::SELECT_LEFT_VERB)
        )
    }

    fn key_exists(sub_key: &str) -> bool {
        hkcu().open_subkey(sub_key).is_ok()
    }

    pub fn is_registered() -> bool {
        key_exists(&probe_key())
    }

    fn delete_tree(sub_key: &str) -> io::Result<()> {
        match hkcu().delete_subkey_all(sub_key) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            other => other,
        }
    }

    fn delete_verb_keys() -> io::Result<()> {
        for key in layout::verb_keys() {
            delete_tree(&format!(r"{}\{key}", layout::CLASSES_ROOT))?;
        }
        Ok(())
    }

    fn notify_shell() {
        use windows_sys::Win32::UI::Shell::{SHCNE_ASSOCCHANGED, SHCNF_IDLIST, SHChangeNotify};
        // SAFETY: no item pointers are passed for SHCNE_ASSOCCHANGED.
        unsafe {
            SHChangeNotify(
                SHCNE_ASSOCCHANGED as i32,
                SHCNF_IDLIST,
                std::ptr::null(),
                std::ptr::null(),
            )
        };
    }

    pub fn register() -> Result<(), String> {
        if !cfg!(target_arch = "x86_64") {
            return Err("The Explorer context menu integration is only available on 64-bit Intel/AMD Windows.".into());
        }
        let exe = exe_path()?;
        let dll = expected_dll_path();
        let err = |e: io::Error| fc_core::compare::io_message(&e);
        if !dll.is_file() {
            fs::create_dir_all(dll_dir()).map_err(err)?;
            let mut temp = dll.clone().into_os_string();
            temp.push(fc_core::ops::TEMP_SUFFIX);
            fs::write(&temp, DLL).map_err(err)?;
            fs::rename(&temp, &dll).map_err(err)?;
            info!("Extracted the shell extension to {}", dll.display());
        }
        delete_verb_keys().map_err(err)?;
        let dll_text = dll.display().to_string();
        for v in layout::build(&exe, &dll_text) {
            let (key, _) = hkcu().create_subkey(&v.sub_key).map_err(err)?;
            key.set_value(&v.name, &v.value).map_err(err)?;
        }
        notify_shell();
        info!("Registered Explorer context menu for {exe} with {dll_text}");
        delete_old_dlls(Some(&dll));
        delete_legacy_selection_file();
        Ok(())
    }

    pub fn unregister() -> Result<(), String> {
        let err = |e: io::Error| fc_core::compare::io_message(&e);
        delete_verb_keys().map_err(err)?;
        for clsid in layout::CLSIDS {
            delete_tree(&format!(
                r"{}\{}",
                layout::CLASSES_ROOT,
                layout::clsid_key(clsid)
            ))
            .map_err(err)?;
        }
        delete_tree(layout::LEFT_SELECTION_KEY).map_err(err)?;
        if let Ok(app) =
            hkcu().open_subkey_with_flags(layout::APP_KEY, winreg::enums::KEY_SET_VALUE)
        {
            let _ = app.delete_value(layout::EXE_PATH_VALUE);
        }
        notify_shell();
        info!("Removed Explorer context menu");
        delete_old_dlls(None);
        Ok(())
    }

    /// Re-registers when the entries point to another exe or DLL, or still use the 1.2.0 layout.
    pub fn ensure_current_path() {
        if !is_registered() {
            return;
        }
        if let Some(legacy) = layout::legacy_keys().into_iter().find(|k| key_exists(k)) {
            info!("Legacy Explorer context menu entries found ({legacy}), updating");
            if let Err(e) = register() {
                warn!("Could not update the Explorer context menu: {e}");
            }
            return;
        }
        let exe: Option<String> = hkcu()
            .open_subkey(layout::APP_KEY)
            .and_then(|k| k.get_value(layout::EXE_PATH_VALUE))
            .ok();
        let dll: Option<String> = hkcu()
            .open_subkey(layout::inproc_server_key(layout::COMPARE_CLSID))
            .and_then(|k| k.get_value(""))
            .ok();
        let ic = fc_core::compare::PathComparison::IgnoreCase;
        let current_exe = exe_path().unwrap_or_default();
        let expected = expected_dll_path().display().to_string();
        let up_to_date = exe.as_deref().is_some_and(|e| ic.eq(e, &current_exe))
            && dll
                .as_deref()
                .is_some_and(|d| ic.eq(d, &expected) && std::path::Path::new(d).is_file());
        if up_to_date {
            return;
        }
        info!("Explorer context menu points to {exe:?} / {dll:?}, updating");
        if let Err(e) = register() {
            warn!("Could not update the Explorer context menu: {e}");
        }
    }

    /// Removes DLLs of earlier versions. A DLL that Explorer still has loaded stays until the next try.
    fn delete_old_dlls(keep: Option<&std::path::Path>) {
        let Ok(entries) = fs::read_dir(dll_dir()) else {
            return;
        };
        for e in entries.flatten() {
            let path = e.path();
            let name = e.file_name().to_string_lossy().into_owned();
            if !name.starts_with("FolderCompare.ShellExt.") || keep.is_some_and(|k| k == path) {
                continue;
            }
            match fs::remove_file(&path) {
                Ok(()) => info!("Deleted old shell extension {}", path.display()),
                Err(_) => info!(
                    "Old shell extension {} is still in use, keeping it",
                    path.display()
                ),
            }
        }
    }

    /// Version 1.2.0 kept the left item in a file; the DLL now keeps it in the registry.
    fn delete_legacy_selection_file() {
        let file = crate::settings::settings_dir().join("explorer-left.txt");
        if file.is_file() {
            match fs::remove_file(&file) {
                Ok(()) => info!("Deleted legacy selection file {}", file.display()),
                Err(e) => warn!("Could not delete the legacy selection file: {e}"),
            }
        }
    }
}
