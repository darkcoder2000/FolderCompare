//! File operations: safety checks, planning and executing copies, deletes and renames.

mod executor;
mod planner;
mod safety;

pub use executor::{
    FileOperationExecutor, ItemOutcome, OpError, OperationItemResult, OperationOptions,
    OperationProgress, OperationSummary, OverwriteChoice, OverwriteConflict, OverwriteDecision,
    OverwritePolicy, OverwriteResolver, TEMP_SUFFIX, Trash, is_valid_name,
};
pub use planner::{
    ActionKind, CopyDirection, OperationPlan, PlannedAction, Sides, plan_copy, plan_delete,
    top_level_only,
};
pub use safety::{UnsafePath, ensure_inside_root, is_inside, normalize, validate_roots};
