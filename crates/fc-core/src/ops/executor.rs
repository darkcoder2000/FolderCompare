//! Executes planned actions one after another (an operation queue). Each item failure is recorded and the queue
//! continues; cancellation stops after cleaning up the current file so no partial files remain.

use std::fs::{self, File, FileTimes, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime};

use tracing::{info, warn};

use super::planner::{ActionKind, PlannedAction};
use super::safety::{UnsafePath, ensure_inside_root, normalize};
use crate::compare::{PathComparison, io_message, is_newer};
use crate::model::Side;

/// Suffix of the temporary file a copy is written to before it replaces the target.
pub const TEMP_SUFFIX: &str = ".tmpcopy";
const DIRECTION_UNKNOWN_MESSAGE: &str = "Same timestamp but different content; direction unknown.";
const BUFFER_SIZE: usize = 1 << 20;
const PROGRESS_STEP: u64 = 8 * BUFFER_SIZE as u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum OverwritePolicy {
    #[default]
    Ask,
    Skip,
    Overwrite,
    OverwriteIfNewer,
    KeepBoth,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OperationOptions {
    pub left_root: PathBuf,
    pub right_root: PathBuf,
    pub tolerance: Duration,
    pub overwrite_policy: OverwritePolicy,
    pub delete_to_recycle_bin: bool,
}

impl OperationOptions {
    pub fn new(left_root: impl Into<PathBuf>, right_root: impl Into<PathBuf>) -> Self {
        Self {
            left_root: left_root.into(),
            right_root: right_root.into(),
            tolerance: Duration::from_secs(2),
            overwrite_policy: OverwritePolicy::Ask,
            delete_to_recycle_bin: true,
        }
    }

    pub fn root_for(&self, side: Side) -> &Path {
        match side {
            Side::Left => &self.left_root,
            Side::Right => &self.right_root,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct OverwriteConflict {
    pub source_path: PathBuf,
    pub target_path: PathBuf,
    pub source_size: u64,
    pub source_time: SystemTime,
    pub target_size: u64,
    pub target_time: SystemTime,
    pub target_is_newer: bool,
    pub direction_unknown: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OverwriteChoice {
    Skip,
    Overwrite,
    OverwriteIfNewer,
    KeepBoth,
    Cancel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OverwriteDecision {
    pub choice: OverwriteChoice,
    pub apply_to_all: bool,
}

/// Asks the user what to do with an existing destination file.
pub type OverwriteResolver<'a, 'f> =
    &'a mut (dyn FnMut(&OverwriteConflict) -> OverwriteDecision + 'f);

#[derive(Debug, Clone, PartialEq, Default)]
pub struct OperationProgress {
    pub current_item: String,
    pub file_bytes_done: u64,
    pub file_bytes_total: u64,
    pub total_bytes_done: u64,
    pub total_bytes: u64,
    pub items_done: usize,
    pub items_total: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ItemOutcome {
    Succeeded,
    Skipped,
    Failed,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OperationItemResult {
    pub action: PlannedAction,
    pub outcome: ItemOutcome,
    pub message: Option<String>,
    /// Where the file ended up (differs from the planned target for "keep both").
    pub final_target: Option<PathBuf>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct OperationSummary {
    pub items: Vec<OperationItemResult>,
    pub cancelled: bool,
}

impl OperationSummary {
    fn count(&self, outcome: ItemOutcome) -> usize {
        self.items.iter().filter(|i| i.outcome == outcome).count()
    }

    pub fn succeeded(&self) -> usize {
        self.count(ItemOutcome::Succeeded)
    }

    pub fn skipped(&self) -> usize {
        self.count(ItemOutcome::Skipped)
    }

    pub fn failed(&self) -> usize {
        self.count(ItemOutcome::Failed)
    }

    pub fn to_report(&self) -> String {
        let nl = crate::NEWLINE;
        let mut out = format!(
            "Succeeded: {}   Skipped: {}   Failed: {}{}{nl}",
            self.succeeded(),
            self.skipped(),
            self.failed(),
            if self.cancelled { "   (cancelled)" } else { "" }
        );
        // Grouped by outcome in order of first appearance.
        let mut groups: Vec<ItemOutcome> = Vec::new();
        for i in &self.items {
            if i.outcome != ItemOutcome::Succeeded && !groups.contains(&i.outcome) {
                groups.push(i.outcome);
            }
        }
        for g in groups {
            out.push_str(nl);
            out.push_str(&format!("{g:?}:{nl}"));
            for item in self.items.iter().filter(|i| i.outcome == g) {
                out.push_str(&format!(
                    "  {}{}{nl}",
                    item.action.description(),
                    item.message
                        .as_deref()
                        .map(|m| format!(" - {m}"))
                        .unwrap_or_default()
                ));
            }
        }
        out
    }
}

/// Moves items to the Recycle Bin (Windows) or Trash (Linux, macOS) instead of deleting them.
pub trait Trash: Sync {
    fn delete_file(&self, path: &Path) -> io::Result<()>;
    fn delete_dir(&self, path: &Path) -> io::Result<()>;
    /// False when `path` can't be moved to the trash. The caller then asks before deleting permanently.
    fn can_recycle(&self, _path: &Path) -> bool {
        true
    }
}

/// Why a single item failed or the run stopped.
#[derive(Debug)]
pub enum OpError {
    Io(io::Error),
    Unsafe(UnsafePath),
    InvalidName(String),
    Cancelled,
}

impl std::fmt::Display for OpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OpError::Io(e) => f.write_str(&io_message(e)),
            OpError::Unsafe(u) => f.write_str(&u.0),
            OpError::InvalidName(m) => f.write_str(m),
            OpError::Cancelled => f.write_str("Cancelled"),
        }
    }
}

impl std::error::Error for OpError {}

impl From<io::Error> for OpError {
    fn from(e: io::Error) -> Self {
        OpError::Io(e)
    }
}

impl From<UnsafePath> for OpError {
    fn from(e: UnsafePath) -> Self {
        OpError::Unsafe(e)
    }
}

struct State {
    policy: OverwritePolicy,
    newer_overwrite_confirmed: bool,
    unknown_direction_confirmed: bool,
}

pub struct FileOperationExecutor<'a> {
    trash: &'a dyn Trash,
    /// Test hook: when false, the copied file's modification time is not set (exercises the verification).
    #[doc(hidden)]
    pub set_times: bool,
}

impl<'a> FileOperationExecutor<'a> {
    pub fn new(trash: &'a dyn Trash) -> Self {
        Self {
            trash,
            set_times: true,
        }
    }

    pub fn execute(
        &self,
        actions: &[PlannedAction],
        options: &OperationOptions,
        mut resolver: Option<OverwriteResolver<'_, '_>>,
        mut progress: Option<&mut (dyn FnMut(&OperationProgress) + '_)>,
        cancel: &AtomicBool,
    ) -> OperationSummary {
        let mut summary = OperationSummary::default();
        let mut state = State {
            policy: options.overwrite_policy,
            newer_overwrite_confirmed: false,
            unknown_direction_confirmed: false,
        };
        let total_bytes: u64 = actions
            .iter()
            .filter(|a| a.kind == ActionKind::CopyFile)
            .map(|a| a.size)
            .sum();
        let mut bytes_done = 0u64;

        for (i, action) in actions.iter().enumerate() {
            if cancel.load(Ordering::Relaxed) {
                Self::cancel_rest(&mut summary, &actions[i..], false);
                break;
            }
            let item = action.target_path.display().to_string();
            if let Some(p) = progress.as_deref_mut() {
                p(&OperationProgress {
                    current_item: item.clone(),
                    file_bytes_done: 0,
                    file_bytes_total: action.size,
                    total_bytes_done: bytes_done,
                    total_bytes,
                    items_done: i,
                    items_total: actions.len(),
                });
            }

            let base = bytes_done;
            let mut file_progress = |done: u64| {
                if let Some(p) = progress.as_deref_mut() {
                    p(&OperationProgress {
                        current_item: item.clone(),
                        file_bytes_done: done,
                        file_bytes_total: action.size,
                        total_bytes_done: base + done,
                        total_bytes,
                        items_done: i,
                        items_total: actions.len(),
                    });
                }
            };
            let outcome = match action.kind {
                ActionKind::CopyFile => self.copy_file(
                    action,
                    options,
                    &mut state,
                    resolver.as_deref_mut(),
                    &mut file_progress,
                    cancel,
                ),
                ActionKind::CreateDirectory => Self::create_directory(action, options),
                ActionKind::Delete => self.delete(action, options),
            };
            let result = match outcome {
                Ok(r) => r,
                Err(OpError::Cancelled) => {
                    Self::cancel_rest(&mut summary, &actions[i..], true);
                    break;
                }
                Err(e) => OperationItemResult {
                    action: action.clone(),
                    outcome: ItemOutcome::Failed,
                    message: Some(e.to_string()),
                    final_target: None,
                },
            };
            if action.kind == ActionKind::CopyFile {
                bytes_done += action.size;
            }
            log(
                action,
                result.outcome,
                result.message.as_deref(),
                result.final_target.as_deref(),
            );
            summary.items.push(result);
        }

        if let Some(p) = progress {
            p(&OperationProgress {
                current_item: String::new(),
                file_bytes_done: 0,
                file_bytes_total: 0,
                total_bytes_done: bytes_done,
                total_bytes,
                items_done: actions.len(),
                items_total: actions.len(),
            });
        }
        summary
    }

    fn cancel_rest(summary: &mut OperationSummary, rest: &[PlannedAction], log_current: bool) {
        summary.cancelled = true;
        for a in rest {
            summary.items.push(OperationItemResult {
                action: a.clone(),
                outcome: ItemOutcome::Skipped,
                message: Some("Cancelled".to_string()),
                final_target: None,
            });
        }
        if log_current && let Some(a) = rest.first() {
            log(a, ItemOutcome::Skipped, Some("Cancelled"), None);
        }
    }

    /// Renames an item on one side within its folder; never overwrites. Returns the new path.
    pub fn rename(
        &self,
        options: &OperationOptions,
        side: Side,
        relative_path: &str,
        new_name: &str,
    ) -> Result<PathBuf, OpError> {
        if !is_valid_name(new_name) {
            return Err(OpError::InvalidName(format!(
                "'{new_name}' is not a valid name."
            )));
        }
        let root = options.root_for(side);
        let source = ensure_inside_root(root, &root.join(relative_path))?;
        let parent = source.parent().map(Path::to_path_buf).unwrap_or_default();
        let target = ensure_inside_root(root, &parent.join(new_name))?;
        let result = (|| -> io::Result<()> {
            let same_entry = PathComparison::for_current_os()
                .eq(&source.to_string_lossy(), &target.to_string_lossy());
            if fs::symlink_metadata(&target).is_ok() && !same_entry {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    format!("'{}' already exists.", target.display()),
                ));
            }
            fs::rename(&source, &target)
        })();
        match result {
            Ok(()) => {
                info!(
                    "Rename {} -> {}: Succeeded",
                    source.display(),
                    target.display()
                );
                Ok(target)
            }
            Err(e) => {
                warn!(
                    "Rename {} -> {}: Failed {}",
                    source.display(),
                    target.display(),
                    io_message(&e)
                );
                Err(OpError::Io(e))
            }
        }
    }

    fn copy_file(
        &self,
        action: &PlannedAction,
        options: &OperationOptions,
        state: &mut State,
        resolver: Option<OverwriteResolver<'_, '_>>,
        file_progress: &mut dyn FnMut(u64),
        cancel: &AtomicBool,
    ) -> Result<OperationItemResult, OpError> {
        let done = |outcome, message: Option<&str>, target: Option<&Path>| OperationItemResult {
            action: action.clone(),
            outcome,
            message: message.map(str::to_string),
            final_target: target.map(Path::to_path_buf),
        };
        let source_side = action.target_side.other();
        let source_path = action.source_path.as_deref().unwrap_or(Path::new(""));
        let source = ensure_inside_root(options.root_for(source_side), source_path)?;
        let target_root = options.root_for(action.target_side);
        let mut target = ensure_inside_root(target_root, &action.target_path)?;

        let source_meta = match fs::metadata(&source) {
            Ok(m) if m.is_file() => m,
            _ => {
                return Ok(done(
                    ItemOutcome::Failed,
                    Some("Source no longer exists."),
                    None,
                ));
            }
        };
        if target.is_dir() {
            return Ok(done(
                ItemOutcome::Failed,
                Some("A folder with the same name exists at the destination."),
                None,
            ));
        }
        let source_time = source_meta.modified()?;

        if let Ok(target_meta) = fs::metadata(&target) {
            let target_time = target_meta.modified()?;
            let target_newer = is_newer(target_time, source_time, options.tolerance);
            let unknown = action.direction_unknown;
            let mut choice = to_choice(state.policy);
            // "Newer wins" cannot decide a same-time conflict; only skipping or keeping both is safe without asking.
            let must_ask = state.policy == OverwritePolicy::Ask
                || (target_newer
                    && choice == OverwriteChoice::Overwrite
                    && !state.newer_overwrite_confirmed)
                || (unknown
                    && matches!(
                        choice,
                        OverwriteChoice::Overwrite | OverwriteChoice::OverwriteIfNewer
                    )
                    && !state.unknown_direction_confirmed);
            if must_ask {
                let Some(resolver) = resolver else {
                    return Ok(done(
                        ItemOutcome::Skipped,
                        Some(if unknown {
                            DIRECTION_UNKNOWN_MESSAGE
                        } else {
                            "Destination exists."
                        }),
                        None,
                    ));
                };
                let decision = resolver(&OverwriteConflict {
                    source_path: source.clone(),
                    target_path: target.clone(),
                    source_size: source_meta.len(),
                    source_time,
                    target_size: target_meta.len(),
                    target_time,
                    target_is_newer: target_newer,
                    direction_unknown: unknown,
                });
                if decision.choice == OverwriteChoice::Cancel {
                    return Err(OpError::Cancelled);
                }
                choice = decision.choice;
                if decision.apply_to_all {
                    if state.policy == OverwritePolicy::Ask {
                        state.policy = to_policy(choice);
                    }
                    if target_newer && choice == OverwriteChoice::Overwrite {
                        state.newer_overwrite_confirmed = true;
                    }
                    if unknown && choice == OverwriteChoice::Overwrite {
                        state.unknown_direction_confirmed = true;
                    }
                }
            }

            match choice {
                OverwriteChoice::Skip | OverwriteChoice::Cancel => {
                    return Ok(done(
                        ItemOutcome::Skipped,
                        Some("Destination exists."),
                        None,
                    ));
                }
                OverwriteChoice::OverwriteIfNewer if unknown => {
                    return Ok(done(
                        ItemOutcome::Skipped,
                        Some(DIRECTION_UNKNOWN_MESSAGE),
                        None,
                    ));
                }
                OverwriteChoice::OverwriteIfNewer
                    if !is_newer(source_time, target_time, options.tolerance) =>
                {
                    return Ok(done(
                        ItemOutcome::Skipped,
                        Some("Destination is not older than the source."),
                        None,
                    ));
                }
                OverwriteChoice::KeepBoth => {
                    target = ensure_inside_root(target_root, &unique_name(&target))?;
                }
                _ => {}
            }
        }

        let directory = target.parent().map(Path::to_path_buf).unwrap_or_default();
        let normalized_root = normalize(&target_root.to_string_lossy()).map_err(UnsafePath)?;
        if directory != normalized_root {
            ensure_inside_root(target_root, &directory)?;
        }
        fs::create_dir_all(&directory)?;

        let mut temp = append(&target, TEMP_SUFFIX);
        if fs::symlink_metadata(&temp).is_ok() {
            temp = append(&target, &format!(".{}{TEMP_SUFFIX}", random_hex8()));
        }

        let copied = (|| -> Result<(), OpError> {
            let created = source_meta.created().ok();
            self.copy_content(
                &source,
                &temp,
                source_meta.len(),
                source_time,
                created,
                file_progress,
                cancel,
            )?;
            #[cfg(windows)]
            clear_readonly_file(&target)?;
            fs::rename(&temp, &target)?;
            Ok(())
        })();
        if let Err(e) = copied {
            if fs::symlink_metadata(&temp).is_ok()
                && let Err(rm) = fs::remove_file(&temp)
            {
                warn!(
                    "Could not remove temporary file {}: {}",
                    temp.display(),
                    io_message(&rm)
                );
            }
            return Err(e);
        }

        copy_attributes(&source_meta, &target)?;

        // Verify the result.
        let Ok(check) = fs::metadata(&target) else {
            return Ok(done(
                ItemOutcome::Failed,
                Some("Verification failed: destination does not exist."),
                Some(&target),
            ));
        };
        if check.len() != source_meta.len() {
            let msg = format!(
                "Verification failed: size {} differs from source size {}.",
                check.len(),
                source_meta.len()
            );
            return Ok(done(ItemOutcome::Failed, Some(&msg), Some(&target)));
        }
        let modified = check.modified()?;
        if is_newer(modified, source_time, options.tolerance)
            || is_newer(source_time, modified, options.tolerance)
        {
            return Ok(done(
                ItemOutcome::Failed,
                Some("Verification failed: timestamp was not preserved."),
                Some(&target),
            ));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let (s, t) = (source_meta.mode() & 0o7777, check.mode() & 0o7777);
            if s != t {
                let msg = format!(
                    "Verification failed: permissions {t:o} differ from the source ({s:o})."
                );
                return Ok(done(ItemOutcome::Failed, Some(&msg), Some(&target)));
            }
        }

        Ok(done(ItemOutcome::Succeeded, None, Some(&target)))
    }

    #[allow(clippy::too_many_arguments)]
    fn copy_content(
        &self,
        source: &Path,
        temp: &Path,
        length: u64,
        modified: SystemTime,
        created: Option<SystemTime>,
        file_progress: &mut dyn FnMut(u64),
        cancel: &AtomicBool,
    ) -> Result<(), OpError> {
        let mut input = File::open(source)?;
        let mut output = OpenOptions::new().write(true).create_new(true).open(temp)?;
        if length > 0 {
            output.set_len(length)?;
        }
        let mut buffer = vec![0u8; (length.clamp(1, BUFFER_SIZE as u64)) as usize];
        let (mut done, mut last_report) = (0u64, 0u64);
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err(OpError::Cancelled);
            }
            let read = match input.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => n,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e.into()),
            };
            output.write_all(&buffer[..read])?;
            done += read as u64;
            if done - last_report >= PROGRESS_STEP {
                last_report = done;
                file_progress(done);
            }
        }
        if done != length {
            output.set_len(done)?;
        }
        file_progress(done);
        if self.set_times {
            output.set_times(FileTimes::new().set_modified(modified))?;
            if let Some(c) = created {
                set_created(&output, c);
            }
        }
        Ok(())
    }

    fn create_directory(
        action: &PlannedAction,
        options: &OperationOptions,
    ) -> Result<OperationItemResult, OpError> {
        let target = ensure_inside_root(options.root_for(action.target_side), &action.target_path)?;
        if target.is_file() {
            return Ok(OperationItemResult {
                action: action.clone(),
                outcome: ItemOutcome::Failed,
                message: Some("A file with the same name exists at the destination.".into()),
                final_target: None,
            });
        }
        fs::create_dir_all(&target)?;
        Ok(OperationItemResult {
            action: action.clone(),
            outcome: ItemOutcome::Succeeded,
            message: None,
            final_target: Some(target),
        })
    }

    fn delete(
        &self,
        action: &PlannedAction,
        options: &OperationOptions,
    ) -> Result<OperationItemResult, OpError> {
        let target = ensure_inside_root(options.root_for(action.target_side), &action.target_path)?;
        let skipped = |msg: &str| OperationItemResult {
            action: action.clone(),
            outcome: ItemOutcome::Skipped,
            message: Some(msg.to_string()),
            final_target: None,
        };
        let exists = fs::symlink_metadata(&target).is_ok();
        if action.is_dir {
            if !exists || !target.is_dir() {
                return Ok(skipped("Folder no longer exists."));
            }
            if options.delete_to_recycle_bin {
                self.trash.delete_dir(&target)?;
            } else {
                #[cfg(windows)]
                clear_readonly_tree(&target)?;
                fs::remove_dir_all(&target)?;
            }
        } else {
            if !exists || target.is_dir() {
                return Ok(skipped("File no longer exists."));
            }
            if options.delete_to_recycle_bin {
                self.trash.delete_file(&target)?;
            } else {
                #[cfg(windows)]
                clear_readonly_file(&target)?;
                fs::remove_file(&target)?;
            }
        }
        Ok(OperationItemResult {
            action: action.clone(),
            outcome: ItemOutcome::Succeeded,
            message: None,
            final_target: Some(target),
        })
    }
}

fn log(
    action: &PlannedAction,
    outcome: ItemOutcome,
    message: Option<&str>,
    final_target: Option<&Path>,
) {
    let source = action
        .source_path
        .as_deref()
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    let target = final_target.unwrap_or(&action.target_path).display();
    let message = message.unwrap_or_default();
    let kind = action.kind;
    if outcome == ItemOutcome::Failed {
        warn!("{kind:?} {source} -> {target}: {outcome:?} {message}");
    } else {
        info!("{kind:?} {source} -> {target}: {outcome:?} {message}");
    }
}

fn to_choice(policy: OverwritePolicy) -> OverwriteChoice {
    match policy {
        OverwritePolicy::Overwrite => OverwriteChoice::Overwrite,
        OverwritePolicy::OverwriteIfNewer => OverwriteChoice::OverwriteIfNewer,
        OverwritePolicy::KeepBoth => OverwriteChoice::KeepBoth,
        OverwritePolicy::Skip | OverwritePolicy::Ask => OverwriteChoice::Skip,
    }
}

fn to_policy(choice: OverwriteChoice) -> OverwritePolicy {
    match choice {
        OverwriteChoice::Overwrite => OverwritePolicy::Overwrite,
        OverwriteChoice::OverwriteIfNewer => OverwritePolicy::OverwriteIfNewer,
        OverwriteChoice::KeepBoth => OverwritePolicy::KeepBoth,
        OverwriteChoice::Skip | OverwriteChoice::Cancel => OverwritePolicy::Skip,
    }
}

/// True for a single valid file name on this OS.
pub fn is_valid_name(name: &str) -> bool {
    if name.trim().is_empty() || name == "." || name == ".." || name.contains('\0') {
        return false;
    }
    let invalid: &[char] = if cfg!(windows) {
        &['<', '>', ':', '"', '/', '\\', '|', '?', '*']
    } else {
        &['/']
    };
    !name.contains(invalid) && !(cfg!(windows) && name.chars().any(|c| (c as u32) < 32))
}

fn append(path: &Path, suffix: &str) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(suffix);
    PathBuf::from(s)
}

/// `name (i).ext` with the first free `i >= 1`.
fn unique_name(path: &Path) -> PathBuf {
    let dir = path.parent().unwrap_or(Path::new(""));
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let ext = path
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    (1..)
        .map(|i| dir.join(format!("{stem} ({i}){ext}")))
        .find(|c| fs::symlink_metadata(c).is_err())
        .expect("a free name exists")
}

fn random_hex8() -> String {
    use std::hash::{BuildHasher, RandomState};
    let n = RandomState::new().hash_one(SystemTime::now());
    format!("{:08x}", n as u32)
}

#[cfg(any(windows, target_vendor = "apple"))]
fn set_created(file: &File, created: SystemTime) {
    #[cfg(target_vendor = "apple")]
    use std::os::darwin::fs::FileTimesExt;
    #[cfg(windows)]
    use std::os::windows::fs::FileTimesExt;
    // Creation time is best effort.
    let _ = file.set_times(FileTimes::new().set_created(created));
}

/// Linux can't set the birth time.
#[cfg(not(any(windows, target_vendor = "apple")))]
fn set_created(_file: &File, _created: SystemTime) {}

#[cfg(windows)]
fn clear_readonly_file(path: &Path) -> io::Result<()> {
    if let Ok(meta) = fs::symlink_metadata(path) {
        let mut perms = meta.permissions();
        if perms.readonly() {
            #[allow(clippy::permissions_set_readonly_false)]
            // Windows: only clears the read-only attribute
            perms.set_readonly(false);
            fs::set_permissions(path, perms)?;
        }
    }
    Ok(())
}

/// Clears read-only below a folder before a permanent delete; never descends into links.
#[cfg(windows)]
fn clear_readonly_tree(dir: &Path) -> io::Result<()> {
    if fs::symlink_metadata(dir)?.file_type().is_symlink() {
        return Ok(());
    }
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let ft = entry.file_type()?;
        if ft.is_symlink() {
            continue;
        }
        if ft.is_dir() {
            clear_readonly_tree(&entry.path())?;
        } else {
            clear_readonly_file(&entry.path())?;
        }
    }
    Ok(())
}

/// Copies read-only, hidden, system and archive (Windows) or the permission bits (Unix) to the target.
#[cfg(windows)]
fn copy_attributes(source_meta: &fs::Metadata, target: &Path) -> io::Result<()> {
    use crate::model::Attributes as A;
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::fs::MetadataExt;
    use windows_sys::Win32::Storage::FileSystem::SetFileAttributesW;
    let wanted = source_meta.file_attributes() & (A::READONLY | A::HIDDEN | A::SYSTEM | A::ARCHIVE);
    if wanted == 0 {
        return Ok(());
    }
    // Long paths need the verbatim prefix for a raw Win32 call (std adds it itself).
    let text = target.to_string_lossy();
    let verbatim = if text.starts_with(r"\\?\") {
        target.as_os_str().to_owned()
    } else if let Some(unc) = text.strip_prefix(r"\\") {
        format!(r"\\?\UNC\{unc}").into()
    } else {
        format!(r"\\?\{text}").into()
    };
    let wide: Vec<u16> = verbatim.encode_wide().chain(Some(0)).collect();
    // SAFETY: `wide` is a NUL-terminated UTF-16 path that lives for the call.
    if unsafe { SetFileAttributesW(wide.as_ptr(), wanted) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(unix)]
fn copy_attributes(source_meta: &fs::Metadata, target: &Path) -> io::Result<()> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    fs::set_permissions(
        target,
        fs::Permissions::from_mode(source_meta.mode() & 0o7777),
    )
}
