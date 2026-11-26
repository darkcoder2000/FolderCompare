//! Guards against operating outside the chosen roots and against invalid root combinations.

use std::path::{Component, MAIN_SEPARATOR, Path, PathBuf};

use crate::compare::PathComparison;

/// A path that must not be touched: outside its root, or not a valid path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnsafePath(pub String);

impl std::fmt::Display for UnsafePath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for UnsafePath {}

/// Absolute path with `.` and `..` resolved lexically (links are not resolved) and no trailing separator; a bare
/// root (`C:\`, `/`) keeps its separator. Errors carry the reason.
pub fn normalize(path: &str) -> Result<PathBuf, String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("The path is empty.".to_string());
    }
    if trimmed.contains('\0') {
        return Err("The path contains a null character.".to_string());
    }
    let absolute =
        std::path::absolute(Path::new(trimmed)).map_err(|e| crate::compare::io_message(&e))?;
    Ok(lexical(&absolute))
}

fn lexical(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::Prefix(_) | Component::RootDir => out.push(c.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                // Never climbs above the root.
                if out.parent().is_some() {
                    out.pop();
                }
            }
            Component::Normal(n) => out.push(n),
        }
    }
    out
}

/// Returns an error message, or `None` if the pair of roots may be compared.
pub fn validate_roots(left: &str, right: &str) -> Option<String> {
    if left.trim().is_empty() || right.trim().is_empty() {
        return Some("Please choose both folders.".to_string());
    }
    let (l, r) = match (normalize(left), normalize(right)) {
        (Ok(l), Ok(r)) => (l, r),
        (Err(e), _) | (_, Err(e)) => return Some(format!("Invalid path: {e}")),
    };
    if !l.is_dir() {
        return Some(format!(
            "Left folder does not exist or is not accessible: {}",
            l.display()
        ));
    }
    if !r.is_dir() {
        return Some(format!(
            "Right folder does not exist or is not accessible: {}",
            r.display()
        ));
    }
    // Ignoring case rejects more pairs, which is the safe direction for these checks on every OS.
    let ic = PathComparison::IgnoreCase;
    if ic.eq(&l.to_string_lossy(), &r.to_string_lossy()) {
        return Some("Left and right are the same folder.".to_string());
    }
    if is_inside_with(&l, &r, ic) || is_inside_with(&r, &l, ic) {
        return Some("One folder is inside the other. Choose two independent folders.".to_string());
    }
    None
}

/// True if `full_path` is strictly below `root` (both already normalized). The comparison is ordinal: it accepts
/// fewer paths than ignoring case, so it is also safe on case-sensitive file systems. Paths handed to the executor
/// are built from the root, so their case matches.
pub fn is_inside(root: &Path, full_path: &Path) -> bool {
    is_inside_with(root, full_path, PathComparison::CaseSensitive)
}

fn is_inside_with(root: &Path, full_path: &Path, comparison: PathComparison) -> bool {
    let root = root.to_string_lossy();
    let full = full_path.to_string_lossy();
    let prefix = if root.ends_with(MAIN_SEPARATOR) {
        root.into_owned()
    } else {
        format!("{root}{MAIN_SEPARATOR}")
    };
    let prefix_len = prefix.chars().count();
    if full.chars().count() <= prefix_len {
        return false;
    }
    let head: String = full.chars().take(prefix_len).collect();
    comparison.eq(&head, &prefix)
}

/// Resolves `path` (`..` included) and fails unless it lies strictly inside `root`. Returns the resolved path.
pub fn ensure_inside_root(root: &Path, path: &Path) -> Result<PathBuf, UnsafePath> {
    let text = path.to_string_lossy();
    let full = normalize(&text).map_err(|e| UnsafePath(format!("Invalid path '{text}': {e}")))?;
    let normalized_root = normalize(&root.to_string_lossy())
        .map_err(|e| UnsafePath(format!("Invalid path '{}': {e}", root.display())))?;
    if !is_inside(&normalized_root, &full) {
        return Err(UnsafePath(format!(
            "Refusing to operate on '{}' because it is outside the root '{}'.",
            full.display(),
            normalized_root.display()
        )));
    }
    Ok(full)
}
