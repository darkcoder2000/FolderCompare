use std::time::{Duration, SystemTime, UNIX_EPOCH};

use fc_core::compare::compare_times;
use fc_core::model::DiffStatus::{self, *};

fn t() -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(1_704_067_200) // 2024-01-01
}

const TOL: Duration = Duration::from_secs(2);

fn plus_ms(ms: i64) -> SystemTime {
    if ms >= 0 {
        t() + Duration::from_millis(ms as u64)
    } else {
        t() - Duration::from_millis((-ms) as u64)
    }
}

#[test]
fn equal_is_identical() {
    assert_eq!(Identical, compare_times(t(), t(), TOL, false));
}

#[test]
fn exactly_at_tolerance_is_identical() {
    assert_eq!(Identical, compare_times(plus_ms(2000), t(), TOL, false));
    assert_eq!(Identical, compare_times(t(), plus_ms(2000), TOL, false));
}

#[test]
fn just_below_tolerance_is_identical() {
    assert_eq!(Identical, compare_times(plus_ms(1999), t(), TOL, false));
}

#[test]
fn just_above_tolerance_is_different() {
    assert_eq!(NewerLeft, compare_times(plus_ms(2001), t(), TOL, false));
    assert_eq!(NewerRight, compare_times(t(), plus_ms(2001), TOL, false));
}

#[test]
fn zero_tolerance_detects_any_difference() {
    assert_eq!(
        NewerLeft,
        compare_times(t() + Duration::from_nanos(100), t(), Duration::ZERO, false)
    );
}

#[test]
fn dst_handling() {
    let cases: [(i64, bool, DiffStatus); 9] = [
        (3600, false, NewerLeft),
        (3600, true, Identical),
        (-3600, true, Identical),
        (3602, true, Identical),
        (3598, true, Identical),
        (3603, true, NewerLeft),
        (-3603, true, NewerRight),
        (1800, true, NewerLeft),
        (7200, true, NewerLeft),
    ];
    for (offset, ignore_dst, expected) in cases {
        assert_eq!(
            expected,
            compare_times(plus_ms(offset * 1000), t(), TOL, ignore_dst),
            "offset {offset}, ignore_dst {ignore_dst}"
        );
    }
}
