use std::path::{MAIN_SEPARATOR, MAIN_SEPARATOR_STR};

use regex::{Regex, RegexBuilder};

/// Matches semicolon-separated glob patterns (`*`, `?`, `**`) case-insensitively. Patterns may use `/` or `\` on
/// every OS; both mean the OS separator, like [`crate::model::DiffNode::relative_path`]. Patterns without a
/// separator match the item name; patterns with one match the relative path.
#[derive(Debug, Clone)]
pub struct PathPatternMatcher {
    include: Vec<Pattern>,
    exclude: Vec<Pattern>,
}

#[derive(Debug, Clone)]
struct Pattern {
    regex: Regex,
    match_path: bool,
}

impl PathPatternMatcher {
    pub fn new(include_patterns: &str, exclude_patterns: &str) -> Self {
        Self {
            include: parse(include_patterns),
            exclude: parse(exclude_patterns),
        }
    }

    pub fn is_excluded(&self, name: &str, relative_path: &str) -> bool {
        self.exclude.iter().any(|p| p.is_match(name, relative_path))
    }

    /// Include patterns only restrict files; folders are always traversed.
    pub fn is_included_file(&self, name: &str, relative_path: &str) -> bool {
        self.include.is_empty() || self.include.iter().any(|p| p.is_match(name, relative_path))
    }
}

fn parse(patterns: &str) -> Vec<Pattern> {
    patterns
        .split(';')
        .map(str::trim)
        .map(|p| p.replace(['/', '\\'], MAIN_SEPARATOR_STR))
        .map(|p| p.trim_matches(MAIN_SEPARATOR).to_string())
        .filter(|p| !p.is_empty())
        .map(|p| Pattern::new(&p))
        .collect()
}

impl Pattern {
    fn new(glob: &str) -> Self {
        let regex = RegexBuilder::new(&to_regex(glob))
            .case_insensitive(true)
            .build()
            .expect("an escaped glob is a valid regex");
        Self {
            regex,
            match_path: glob.contains(MAIN_SEPARATOR),
        }
    }

    fn is_match(&self, name: &str, relative_path: &str) -> bool {
        self.regex
            .is_match(if self.match_path { relative_path } else { name })
    }
}

fn to_regex(glob: &str) -> String {
    let not_sep = format!("[^{}]", regex::escape(MAIN_SEPARATOR_STR));
    let mut out = String::from("^");
    let chars: Vec<char> = glob.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '*' if chars.get(i + 1) == Some(&'*') => {
                out.push_str(".*");
                i += 1;
            }
            '*' => {
                out.push_str(&not_sep);
                out.push('*');
            }
            '?' => out.push_str(&not_sep),
            c => out.push_str(&regex::escape(c.encode_utf8(&mut [0; 4]))),
        }
        i += 1;
    }
    out.push('$');
    out
}
