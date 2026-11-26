//! `settings.json`, shared with the WPF app: PascalCase names, enums as strings, missing fields take their
//! defaults, unknown fields are kept when writing back. Written atomically through `settings.json.tmp`.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use fc_core::model::{CompareOptions, ContentCompareMode, SymlinkMode};
use fc_core::ops::OverwritePolicy;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value};

/// Reads a value and falls back to the default when it has an unexpected shape (e.g. an unknown enum name).
fn lenient<'de, D, T>(d: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned + Default,
{
    let v = Value::deserialize(d)?;
    Ok(T::deserialize(v).unwrap_or_default())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ContentCompareSetting {
    #[default]
    Off,
    WhenTimestampsEqual,
    WhenSizesEqual,
}

impl From<ContentCompareSetting> for ContentCompareMode {
    fn from(v: ContentCompareSetting) -> Self {
        match v {
            ContentCompareSetting::Off => ContentCompareMode::Off,
            ContentCompareSetting::WhenTimestampsEqual => ContentCompareMode::WhenTimestampsEqual,
            ContentCompareSetting::WhenSizesEqual => ContentCompareMode::WhenSizesEqual,
        }
    }
}

impl From<ContentCompareMode> for ContentCompareSetting {
    fn from(v: ContentCompareMode) -> Self {
        match v {
            ContentCompareMode::Off => ContentCompareSetting::Off,
            ContentCompareMode::WhenTimestampsEqual => ContentCompareSetting::WhenTimestampsEqual,
            ContentCompareMode::WhenSizesEqual => ContentCompareSetting::WhenSizesEqual,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SymlinkSetting {
    #[default]
    Skip,
    Follow,
}

impl From<SymlinkSetting> for SymlinkMode {
    fn from(v: SymlinkSetting) -> Self {
        match v {
            SymlinkSetting::Skip => SymlinkMode::Skip,
            SymlinkSetting::Follow => SymlinkMode::Follow,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum OverwriteSetting {
    #[default]
    Ask,
    Skip,
    Overwrite,
    OverwriteIfNewer,
    KeepBoth,
}

impl From<OverwriteSetting> for OverwritePolicy {
    fn from(v: OverwriteSetting) -> Self {
        match v {
            OverwriteSetting::Ask => OverwritePolicy::Ask,
            OverwriteSetting::Skip => OverwritePolicy::Skip,
            OverwriteSetting::Overwrite => OverwritePolicy::Overwrite,
            OverwriteSetting::OverwriteIfNewer => OverwritePolicy::OverwriteIfNewer,
            OverwriteSetting::KeepBoth => OverwritePolicy::KeepBoth,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct FilterSettings {
    pub show_only_left: bool,
    pub show_only_right: bool,
    pub show_newer_left: bool,
    pub show_newer_right: bool,
    pub show_identical: bool,
    pub show_same_time_different: bool,
    pub show_time_only: bool,
    pub show_errors: bool,
    pub differences_only: bool,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Default for FilterSettings {
    fn default() -> Self {
        Self {
            show_only_left: true,
            show_only_right: true,
            show_newer_left: true,
            show_newer_right: true,
            show_identical: true,
            show_same_time_different: true,
            show_time_only: true,
            show_errors: true,
            differences_only: false,
            extra: Map::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct WindowSettings {
    pub left: f64,
    pub top: f64,
    pub width: f64,
    pub height: f64,
    pub maximized: bool,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// A named folder pair plus the comparison options and filters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct Profile {
    pub name: String,
    pub left_path: String,
    pub right_path: String,
    pub tolerance_seconds: f64,
    pub ignore_dst_offset: bool,
    #[serde(deserialize_with = "lenient")]
    pub content_compare: ContentCompareSetting,
    pub ignore_hidden_and_system: bool,
    pub recursive: bool,
    pub exclude_patterns: String,
    pub include_patterns: String,
    pub filters: FilterSettings,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            name: String::new(),
            left_path: String::new(),
            right_path: String::new(),
            tolerance_seconds: 2.0,
            ignore_dst_offset: false,
            content_compare: ContentCompareSetting::Off,
            ignore_hidden_and_system: false,
            recursive: true,
            exclude_patterns: String::new(),
            include_patterns: String::new(),
            filters: FilterSettings::default(),
            extra: Map::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct TextCompareSettings {
    pub ignore_whitespace: bool,
    pub ignore_case: bool,
    pub show_whitespace: bool,
    pub window: Option<WindowSettings>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct AppSettings {
    // Comparison
    pub tolerance_seconds: f64,
    pub ignore_dst_offset: bool,
    #[serde(deserialize_with = "lenient")]
    pub content_compare: ContentCompareSetting,
    #[serde(deserialize_with = "lenient")]
    pub symlink_mode: SymlinkSetting,
    pub ignore_hidden_and_system: bool,
    pub recursive: bool,
    pub exclude_patterns: String,
    pub include_patterns: String,

    // Operations
    #[serde(deserialize_with = "lenient")]
    pub overwrite_policy: OverwriteSetting,
    pub delete_to_recycle_bin: bool,
    pub confirm_recycle_delete: bool,
    /// The copy preview is skipped when the batch has fewer than this many actions (0 = always preview).
    pub preview_threshold: i64,

    // View
    pub folders_first: bool,
    pub filters: FilterSettings,

    // Session
    pub last_left: String,
    pub last_right: String,
    pub recent_paths: Vec<String>,
    pub window: Option<WindowSettings>,
    pub column_widths: Option<Vec<f64>>,
    pub profiles: Vec<Profile>,
    pub text_compare: TextCompareSettings,

    /// Fields written by other versions of the app, kept unchanged.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            tolerance_seconds: 2.0,
            ignore_dst_offset: false,
            content_compare: ContentCompareSetting::Off,
            symlink_mode: SymlinkSetting::Skip,
            ignore_hidden_and_system: false,
            recursive: true,
            exclude_patterns: String::new(),
            include_patterns: String::new(),
            overwrite_policy: OverwriteSetting::Ask,
            delete_to_recycle_bin: true,
            confirm_recycle_delete: true,
            preview_threshold: 0,
            folders_first: true,
            filters: FilterSettings::default(),
            last_left: String::new(),
            last_right: String::new(),
            recent_paths: Vec::new(),
            window: None,
            column_widths: None,
            profiles: Vec::new(),
            text_compare: TextCompareSettings::default(),
            extra: Map::new(),
        }
    }
}

impl AppSettings {
    pub fn tolerance(&self) -> Duration {
        Duration::from_secs_f64(self.tolerance_seconds.max(0.0))
    }

    pub fn to_compare_options(&self) -> CompareOptions {
        CompareOptions {
            recursive: self.recursive,
            tolerance: self.tolerance(),
            ignore_dst_offset: self.ignore_dst_offset,
            symlink_mode: self.symlink_mode.into(),
            include_patterns: self.include_patterns.clone(),
            exclude_patterns: self.exclude_patterns.clone(),
            ignore_hidden_and_system: self.ignore_hidden_and_system,
            content_compare: self.content_compare.into(),
            path_comparison: None,
        }
    }

    pub fn from_json(text: &str) -> serde_json::Result<Self> {
        serde_json::from_str(text)
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("settings serialize")
    }
}

/// `%APPDATA%\FolderCompare` (Windows), `~/.config/FolderCompare` (Linux),
/// `~/Library/Application Support/FolderCompare` (macOS). `FOLDERCOMPARE_SETTINGS_DIR` overrides it (testing).
pub fn settings_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("FOLDERCOMPARE_SETTINGS_DIR") {
        return PathBuf::from(dir);
    }
    dirs::config_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("FolderCompare")
}

pub fn settings_path() -> PathBuf {
    settings_dir().join("settings.json")
}

/// Loads the settings; defaults when the file is missing or unreadable.
pub fn load(path: &Path) -> AppSettings {
    match fs::read_to_string(path) {
        Ok(text) => {
            let text = text.trim_start_matches('\u{feff}');
            match AppSettings::from_json(text) {
                Ok(s) => s,
                Err(e) => {
                    tracing::error!("Could not read settings, using defaults: {e}");
                    AppSettings::default()
                }
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => AppSettings::default(),
        Err(e) => {
            tracing::error!("Could not read settings, using defaults: {e}");
            AppSettings::default()
        }
    }
}

/// Writes the settings through a temporary file, so a crash never leaves a half-written file.
pub fn save(path: &Path, settings: &AppSettings) {
    let result = (|| -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let mut temp = path.as_os_str().to_owned();
        temp.push(".tmp");
        fs::write(&temp, settings.to_json())?;
        fs::rename(&temp, path)
    })();
    if let Err(e) = result {
        tracing::error!("Could not save settings: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_wpf_app() {
        let s = AppSettings::default();
        assert_eq!(2.0, s.tolerance_seconds);
        assert!(
            s.recursive && s.delete_to_recycle_bin && s.confirm_recycle_delete && s.folders_first
        );
        assert_eq!(OverwriteSetting::Ask, s.overwrite_policy);
        assert!(s.filters.show_identical && !s.filters.differences_only);
        assert_eq!(Duration::from_secs(2), s.to_compare_options().tolerance);
    }

    #[test]
    fn reads_wpf_json_with_missing_and_unknown_fields() {
        let json = r#"{
            "ToleranceSeconds": 3.5,
            "ContentCompare": "WhenSizesEqual",
            "OverwritePolicy": "NoSuchPolicy",
            "Filters": { "ShowIdentical": false },
            "Window": { "Left": 10, "Top": 20, "Width": 800, "Height": 600, "Maximized": true },
            "ColumnWidths": [320, 80, 135, 36, 320, 80, 135],
            "Profiles": [ { "Name": "p", "LeftPath": "a", "SymlinkMode": "Follow" } ],
            "FutureSetting": { "x": 1 }
        }"#;
        let s = AppSettings::from_json(json).unwrap();
        assert_eq!(3.5, s.tolerance_seconds);
        assert_eq!(ContentCompareSetting::WhenSizesEqual, s.content_compare);
        assert_eq!(OverwriteSetting::Ask, s.overwrite_policy);
        assert!(!s.filters.show_identical && s.filters.show_errors);
        assert!(s.window.as_ref().unwrap().maximized);
        assert_eq!(7, s.column_widths.as_ref().unwrap().len());
        assert_eq!("p", s.profiles[0].name);
        assert_eq!(2.0, s.profiles[0].tolerance_seconds);
        assert!(s.recursive);

        let written = s.to_json();
        assert!(written.contains("\"FutureSetting\""));
        assert!(written.contains("\"ContentCompare\": \"WhenSizesEqual\""));
        assert!(written.contains("\"SymlinkMode\": \"Follow\""));
        assert_eq!(s, AppSettings::from_json(&written).unwrap());
    }

    #[test]
    fn save_and_load_round_trip() {
        let dir = std::env::temp_dir().join(format!("fc-settings-{}", std::process::id()));
        let path = dir.join("settings.json");
        let mut s = AppSettings {
            recent_paths: vec!["a".into(), "b".into()],
            ..AppSettings::default()
        };
        s.text_compare.ignore_case = true;
        save(&path, &s);
        assert_eq!(s, load(&path));
        assert!(!dir.join("settings.json.tmp").exists());
        let _ = fs::remove_dir_all(&dir);
        assert_eq!(AppSettings::default(), load(&path));
    }
}
