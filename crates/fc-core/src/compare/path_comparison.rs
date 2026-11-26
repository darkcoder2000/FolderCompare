use std::borrow::Cow;
use std::path::Path;

/// How file and folder names are matched between the two trees: case-insensitive on Windows and macOS (default file
/// systems), case-sensitive on Linux. On Linux `a.txt` and `A.txt` are two different files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PathComparison {
    IgnoreCase,
    CaseSensitive,
}

/// Upper-cases one char 1:1 (chars whose upper case is several chars stay as they are), like an ordinal
/// case-insensitive comparison.
pub fn upper_char(c: char) -> char {
    if c.is_ascii() {
        return c.to_ascii_uppercase();
    }
    let mut up = c.to_uppercase();
    match (up.next(), up.next()) {
        (Some(u), None) => u,
        _ => c,
    }
}

impl PathComparison {
    /// The usual rule of the current operating system.
    pub fn for_current_os() -> Self {
        if cfg!(target_os = "linux") {
            PathComparison::CaseSensitive
        } else {
            PathComparison::IgnoreCase
        }
    }

    /// The OS default, made case-sensitive when one of the roots turns out to be on a case-sensitive file system
    /// (e.g. a case-sensitive APFS volume or a case-sensitive folder on Windows). Case-sensitive is the stricter
    /// rule: it never merges two different files into one row.
    pub fn for_roots(roots: &[&Path]) -> Self {
        let result = Self::for_current_os();
        if !result.is_ignore_case() {
            return result;
        }
        if roots.iter().any(|r| is_case_sensitive(r)) {
            PathComparison::CaseSensitive
        } else {
            result
        }
    }

    pub fn is_ignore_case(self) -> bool {
        self == PathComparison::IgnoreCase
    }

    /// A key that is equal for two names exactly when they match.
    pub fn key(self, name: &str) -> Cow<'_, str> {
        match self {
            PathComparison::CaseSensitive => Cow::Borrowed(name),
            PathComparison::IgnoreCase => {
                if name.chars().all(|c| upper_char(c) == c) {
                    Cow::Borrowed(name)
                } else {
                    Cow::Owned(name.chars().map(upper_char).collect())
                }
            }
        }
    }

    pub fn eq(self, a: &str, b: &str) -> bool {
        match self {
            PathComparison::CaseSensitive => a == b,
            PathComparison::IgnoreCase => {
                let mut x = a.chars();
                let mut y = b.chars();
                loop {
                    match (x.next(), y.next()) {
                        (None, None) => return true,
                        (Some(p), Some(q)) if upper_char(p) == upper_char(q) => {}
                        _ => return false,
                    }
                }
            }
        }
    }
}

/// Read-only probe: flips the case of the last path segment and checks whether it still resolves. If it does not,
/// the file system is case-sensitive. Roots without letters can't be probed and keep the OS default.
fn is_case_sensitive(root: &Path) -> bool {
    let Ok(full) = crate::ops::normalize(&root.to_string_lossy()) else {
        return false;
    };
    let Some(name) = full.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    let flipped: String = name
        .chars()
        .map(|c| {
            if c.is_uppercase() {
                let mut low = c.to_lowercase();
                match (low.next(), low.next()) {
                    (Some(l), None) => l,
                    _ => c,
                }
            } else {
                upper_char(c)
            }
        })
        .collect();
    if flipped == name {
        return false;
    }
    let probe = full.with_file_name(flipped);
    full.is_dir() && !probe.is_dir()
}
