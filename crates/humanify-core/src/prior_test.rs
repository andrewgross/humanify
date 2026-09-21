//! Tests for the prior-version input contract (`crate::prior`).

use crate::prior::{
    SAME_PROGRAM_FLOOR_MIN_FUNCTIONS, SAME_PROGRAM_PRESENCE_FLOOR,
    assert_prior_looks_like_same_program,
};

#[test]
fn passes_when_every_prior_function_is_present() {
    assert_prior_looks_like_same_program(100, 0).unwrap();
}

#[test]
fn passes_when_presence_clears_the_floor() {
    // 5% of 100 = 5 present exactly — the boundary passes.
    let present = (SAME_PROGRAM_PRESENCE_FLOOR * 100.0) as usize;
    assert_prior_looks_like_same_program(100, 100 - present).unwrap();
}

#[test]
fn fails_when_presence_misses_the_floor() {
    let result = assert_prior_looks_like_same_program(100, 99);
    let message = result.unwrap_err();
    assert!(
        message.contains("does not appear to be the same program"),
        "wrong message: {message}"
    );
    assert!(message.contains("1 of 100"), "wrong message: {message}");
}

#[test]
fn skips_the_floor_for_a_tiny_prior() {
    // Below the min-functions floor the check does not apply at all — a
    // fixture with one prior function and zero matches must pass.
    assert_prior_looks_like_same_program(
        SAME_PROGRAM_FLOOR_MIN_FUNCTIONS - 1,
        SAME_PROGRAM_FLOOR_MIN_FUNCTIONS - 1,
    )
    .unwrap();
}

#[test]
fn floor_applies_exactly_at_the_min_functions_boundary() {
    assert_prior_looks_like_same_program(SAME_PROGRAM_FLOOR_MIN_FUNCTIONS, 100).unwrap_err();
}
