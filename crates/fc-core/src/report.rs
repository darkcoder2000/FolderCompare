//! Comparison reports as CSV or plain text. The app adds the UTF-8 BOM when writing the file.

use std::path::MAIN_SEPARATOR;
use std::time::SystemTime;

use chrono::{DateTime, Local};

use crate::NEWLINE;
use crate::compare::DiffResult;
use crate::model::NodeId;

const DATE_FORMAT: &str = "%Y-%m-%d %H:%M:%S";

/// A timestamp in local time as `yyyy-MM-dd HH:mm:ss`.
pub fn format_date(t: SystemTime) -> String {
    DateTime::<Local>::from(t).format(DATE_FORMAT).to_string()
}

pub fn write_csv(result: &DiffResult, nodes: &[NodeId]) -> String {
    let mut out = format!(
        "RelativePath,Type,Status,LeftSize,LeftModified,RightSize,RightModified,Message{NEWLINE}"
    );
    for &id in nodes {
        let n = result.tree.node(id);
        let fields = [
            csv(&n.relative_path),
            if n.is_dir { "Folder" } else { "File" }.to_string(),
            format!("{:?}", n.status),
            n.left
                .as_ref()
                .map(|_| n.left_total.to_string())
                .unwrap_or_default(),
            n.left
                .as_ref()
                .map(|s| format_date(s.modified))
                .unwrap_or_default(),
            n.right
                .as_ref()
                .map(|_| n.right_total.to_string())
                .unwrap_or_default(),
            n.right
                .as_ref()
                .map(|s| format_date(s.modified))
                .unwrap_or_default(),
            csv(n.error.as_deref().unwrap_or_default()),
        ];
        out.push_str(&fields.join(","));
        out.push_str(NEWLINE);
    }
    out
}

pub fn write_text(result: &DiffResult, nodes: &[NodeId], created: SystemTime) -> String {
    let mut out = String::new();
    let mut line = |s: String| {
        out.push_str(&s);
        out.push_str(NEWLINE);
    };
    line("FolderCompare comparison report".into());
    line(format!("Left:  {}", result.left_root.display()));
    line(format!("Right: {}", result.right_root.display()));
    line(format!("Created: {}", format_date(created)));
    line(String::new());
    for &id in nodes {
        let n = result.tree.node(id);
        let mut s = format!(
            "{:<24} {}{}",
            format!("{:?}", n.status),
            n.relative_path,
            if n.is_dir {
                MAIN_SEPARATOR.to_string()
            } else {
                String::new()
            }
        );
        if let Some(l) = &n.left {
            s.push_str(&format!("  L: {}", format_date(l.modified)));
        }
        if let Some(r) = &n.right {
            s.push_str(&format!("  R: {}", format_date(r.modified)));
        }
        if let Some(e) = &n.error {
            s.push_str(&format!("  ({e})"));
        }
        line(s);
    }
    out
}

fn csv(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}
