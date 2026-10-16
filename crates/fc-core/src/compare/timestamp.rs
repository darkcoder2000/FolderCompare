use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::model::DiffStatus;

const ONE_HOUR_NS: i128 = 3_600_000_000_000;

pub(crate) fn nanos(t: SystemTime) -> i128 {
    match t.duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_nanos() as i128,
        Err(e) => -(e.duration().as_nanos() as i128),
    }
}

/// Compares two timestamps: within `tolerance` (inclusive) they are identical; with `ignore_dst_offset` a distance
/// of one hour ± tolerance is identical too. Otherwise the later one is newer.
pub fn compare_times(
    left: SystemTime,
    right: SystemTime,
    tolerance: Duration,
    ignore_dst_offset: bool,
) -> DiffStatus {
    let l = nanos(left);
    let r = nanos(right);
    let diff = (l - r).abs();
    let tol = tolerance.as_nanos() as i128;
    if diff <= tol {
        return DiffStatus::Identical;
    }
    if ignore_dst_offset && (diff - ONE_HOUR_NS).abs() <= tol {
        return DiffStatus::Identical;
    }
    if l > r {
        DiffStatus::NewerLeft
    } else {
        DiffStatus::NewerRight
    }
}

/// True when `a` is newer than `b` by more than the tolerance.
pub fn is_newer(a: SystemTime, b: SystemTime, tolerance: Duration) -> bool {
    nanos(a) - nanos(b) > tolerance.as_nanos() as i128
}
