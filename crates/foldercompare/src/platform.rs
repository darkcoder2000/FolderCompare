//! OS services: trash, opening files and folders, the log folder.

use std::io;
use std::path::{Path, PathBuf};

use fc_core::ops::Trash;

/// Prefix of the error message when an item could not be moved to the trash (the app then offers a permanent
/// delete instead).
pub const TRASH_FAILED: &str = "Could not move to the ";

pub fn trash_name() -> &'static str {
    if cfg!(windows) {
        "Recycle Bin"
    } else {
        "Trash"
    }
}

/// Recycle Bin (Windows), Trash via `NSFileManager` (macOS), freedesktop trash (Linux).
pub struct AppTrash {
    ctx: trash::TrashContext,
}

impl Default for AppTrash {
    fn default() -> Self {
        #[allow(unused_mut)]
        let mut ctx = trash::TrashContext::default();
        #[cfg(target_os = "macos")]
        {
            use trash::macos::{DeleteMethod, TrashContextExtMacos};
            // No Finder automation permission needed, no sound.
            ctx.set_delete_method(DeleteMethod::NsFileManager);
        }
        Self { ctx }
    }
}

impl AppTrash {
    fn delete(&self, path: &Path) -> io::Result<()> {
        self.ctx
            .delete(path)
            .map_err(|e| io::Error::other(format!("{TRASH_FAILED}{}: {e}", trash_name())))
    }
}

impl Trash for AppTrash {
    fn delete_file(&self, path: &Path) -> io::Result<()> {
        self.delete(path)
    }

    fn delete_dir(&self, path: &Path) -> io::Result<()> {
        self.delete(path)
    }
}

/// `%LOCALAPPDATA%\FolderCompare\logs`, `~/.local/state/FolderCompare/logs`, `~/Library/Logs/FolderCompare`.
pub fn log_dir() -> PathBuf {
    if cfg!(target_os = "macos") {
        return dirs::home_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("Library/Logs/FolderCompare");
    }
    let base = if cfg!(windows) {
        dirs::data_local_dir()
    } else {
        dirs::state_dir().or_else(dirs::data_local_dir)
    };
    base.unwrap_or_else(std::env::temp_dir)
        .join("FolderCompare")
        .join("logs")
}

/// Opens a file or folder with its default program.
pub fn open(path: &Path) -> Result<(), String> {
    opener::open(path).map_err(|e| e.to_string())
}

/// Shows the item selected in the file manager.
pub fn reveal(path: &Path) -> Result<(), String> {
    opener::reveal(path).map_err(|e| e.to_string())
}

/// Name of the OS file manager for menu entries.
pub fn file_manager_name() -> &'static str {
    if cfg!(windows) {
        "Explorer"
    } else if cfg!(target_os = "macos") {
        "Finder"
    } else {
        "file manager"
    }
}

/// The primary modifier shown in shortcut texts: Cmd on macOS, Ctrl elsewhere.
pub fn ctrl_name() -> &'static str {
    if cfg!(target_os = "macos") {
        "Cmd"
    } else {
        "Ctrl"
    }
}
