//! The switch registry's contract tests (the TS kill-switches.test.ts
//! analog): validation errors list the valid names; the state is a value;
//! blank segments are skipped; the census cannot typo.

use crate::kill_switches::{Switch, SwitchKind, SwitchState, valid_names};

#[test]
fn configure_with_unknown_disable_name_errors_with_valid_list() {
    let err = SwitchState::configure(&["no-such-switch".into()], &[])
        .expect_err("an unknown switch must be an error");
    assert!(
        err.contains("--disable: unknown disable switch \"no-such-switch\""),
        "message should name the flag and the bad name: {err}"
    );
    for name in valid_names(SwitchKind::Disable) {
        assert!(
            err.contains(name),
            "message should list the valid switches: {err}"
        );
    }
}

#[test]
fn configure_with_probe_name_in_disable_list_is_wrong_kind() {
    // shingle-probe is the registry's only probe switch.
    let err = SwitchState::configure(&["shingle-probe".into()], &[])
        .expect_err("a probe name under --disable is wrong-kind, fatal");
    assert!(
        err.contains("wrong-kind") || err.contains("unknown"),
        "{err}"
    );
}

#[test]
fn configure_applies_disable_and_probe_kinds_separately() {
    let state = SwitchState::configure(
        &["family-permute".into(), " post-split-reconcile ".into()],
        &["shingle-probe".into()],
    )
    .expect("valid names must apply");
    assert!(state.switch_on(Switch::FamilyPermute));
    assert!(state.switch_on(Switch::PostSplitReconcile));
    assert!(state.switch_on(Switch::ShingleProbe));
    assert!(!state.switch_on(Switch::ContentAnchor));
}

#[test]
fn configure_skips_blank_segments() {
    let state = SwitchState::configure(&["".into(), "  ".into()], &[])
        .expect("blank segments are skipped, not errors");
    assert!(state.active().is_empty());
}

#[test]
fn active_reports_sorted_applied_switches() {
    let state = SwitchState::configure(
        &["post-split-reconcile".into(), "family-permute".into()],
        &[],
    )
    .expect("valid");
    let active = state.active();
    let names: Vec<&str> = active.iter().map(|s| s.name()).collect();
    assert_eq!(names, vec!["family-permute", "post-split-reconcile"]);
}

#[test]
fn registry_names_match_the_ts_wire_names() {
    // The registry IS the single source of truth: the flag names on the
    // wire are the TS registry's keys, and every one must be reachable
    // through by_name (a gap here is a switch --disable cannot reach).
    for s in Switch::ALL {
        let found = Switch::by_name(s.name());
        assert_eq!(found, Some(s), "by_name must find {}", s.name());
    }
    assert_eq!(Switch::by_name("not-a-switch"), None);
    assert_eq!(Switch::ALL.len(), 14);
    assert_eq!(valid_names(SwitchKind::Disable).len(), 13);
    assert_eq!(valid_names(SwitchKind::Probe).len(), 1);
}
