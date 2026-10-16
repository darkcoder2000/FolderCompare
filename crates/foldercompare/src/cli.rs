//! `FolderCompare "C:\A" "D:\B" [--compare] [--no-recursive] [--content off|equal-time|equal-size]`.
//! Two files open the text compare window (used by the Explorer "Compare Files" entry).

use fc_core::model::ContentCompareMode;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CommandLineOptions {
    pub left: Option<String>,
    pub right: Option<String>,
    pub compare: bool,
    pub no_recursive: bool,
    pub content_compare: Option<ContentCompareMode>,
}

impl CommandLineOptions {
    /// Parses the arguments (without the program name). Options are case-insensitive; unknown options are ignored.
    pub fn parse<S: AsRef<str>>(args: &[S]) -> Self {
        let mut o = Self::default();
        let mut positional = Vec::new();
        let mut i = 0;
        while i < args.len() {
            let arg = args[i].as_ref();
            let lower = arg.to_lowercase();
            if lower == "--compare" {
                o.compare = true;
            } else if lower == "--no-recursive" {
                o.no_recursive = true;
            } else if lower == "--content" {
                if i + 1 < args.len() {
                    i += 1;
                    o.content_compare = parse_content_mode(args[i].as_ref()).or(o.content_compare);
                }
            } else if let Some(value) = lower.strip_prefix("--content=") {
                o.content_compare = parse_content_mode(value).or(o.content_compare);
            } else if !arg.starts_with("--") {
                positional.push(arg.to_string());
            }
            i += 1;
        }
        let mut positional = positional.into_iter();
        o.left = positional.next();
        o.right = positional.next();
        o
    }

    /// `--compare` only counts when both paths are given.
    pub fn compare_on_startup(&self) -> bool {
        self.compare && self.left.is_some() && self.right.is_some()
    }
}

fn parse_content_mode(value: &str) -> Option<ContentCompareMode> {
    match value.to_lowercase().as_str() {
        "off" => Some(ContentCompareMode::Off),
        "equal-time" => Some(ContentCompareMode::WhenTimestampsEqual),
        "equal-size" => Some(ContentCompareMode::WhenSizesEqual),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_paths_and_flags() {
        let o = CommandLineOptions::parse(&[r"C:\A", r"D:\B", "--COMPARE", "--no-recursive"]);
        assert_eq!(Some(r"C:\A".to_string()), o.left);
        assert_eq!(Some(r"D:\B".to_string()), o.right);
        assert!(o.compare && o.no_recursive && o.compare_on_startup());
        assert_eq!(None, o.content_compare);
    }

    #[test]
    fn content_mode_both_forms() {
        let o = CommandLineOptions::parse(&["--content", "Equal-Time"]);
        assert_eq!(
            Some(ContentCompareMode::WhenTimestampsEqual),
            o.content_compare
        );
        let o = CommandLineOptions::parse(&["--content=equal-size"]);
        assert_eq!(Some(ContentCompareMode::WhenSizesEqual), o.content_compare);
        let o = CommandLineOptions::parse(&["--content=bogus"]);
        assert_eq!(None, o.content_compare);
    }

    #[test]
    fn unknown_options_are_ignored_and_compare_needs_both_paths() {
        let o = CommandLineOptions::parse(&["--verbose", "only-left", "--compare"]);
        assert_eq!(Some("only-left".to_string()), o.left);
        assert_eq!(None, o.right);
        assert!(o.compare);
        assert!(!o.compare_on_startup());
    }
}
