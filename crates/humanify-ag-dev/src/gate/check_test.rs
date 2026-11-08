use std::time::Duration;

use super::*;

fn stage(name: &'static str, advisory: bool) -> Stage {
    Stage {
        name,
        why: "because",
        run: Run::Dev(&["x"]),
        advisory,
        deadline: Duration::from_secs(1),
    }
}

/// Runs `stages`, failing the ones named in `fail`; returns the summary.
fn gate(stages: &[Stage], only: Option<&[String]>, fail: &[&str]) -> (Vec<Row>, u8, String) {
    let mut exec = |s: &Stage| !fail.contains(&s.name);
    let rows = run_stages(stages, only, &mut exec);
    let (code, text) = summary(stages, &rows);
    (rows, code, text)
}

#[test]
fn a_green_run_says_every_stage_passed() {
    let stages = [stage("a", false), stage("b", false)];
    let (_, code, text) = gate(&stages, None, &[]);
    assert_eq!(code, 0);
    assert!(text.ends_with("ALL 2 STAGES PASSED\n"), "{text}");
}

#[test]
fn a_failure_stops_the_gate_and_names_every_stage_it_did_not_reach() {
    let stages = [stage("a", false), stage("b", false), stage("c", false)];
    let (rows, code, text) = gate(&stages, None, &["a"]);
    assert_eq!(code, 1);
    assert_eq!(rows.len(), 1, "fail fast: nothing after a ran");
    assert!(text.contains("FAIL   a"), "{text}");
    assert!(
        text.contains("---    b                      not reached"),
        "{text}"
    );
    assert!(text.contains("FAILED: a — because"), "{text}");
    assert!(
        text.contains("2 later stage(s) did not run. This is NOT a green gate."),
        "{text}"
    );
}

#[test]
fn an_advisory_stage_prints_review_and_the_gate_stays_green() {
    let stages = [stage("a", true), stage("b", false)];
    let (rows, code, text) = gate(&stages, None, &["a"]);
    assert_eq!(code, 0);
    assert_eq!(rows.len(), 2, "later stages still run");
    assert!(text.contains("REVIEW a"), "{text}");
    assert!(text.contains("REVIEW: a — because"), "{text}");
    assert!(text.ends_with("ALL 2 STAGES PASSED\n"), "{text}");
}

#[test]
fn only_is_labelled_partial_and_never_reads_as_a_green_gate() {
    let stages = [stage("a", false), stage("b", false)];
    let only = vec!["b".to_string()];
    let (_, code, text) = gate(&stages, Some(&only), &[]);
    assert_eq!(code, 0);
    assert!(text.contains("skip   a"), "{text}");
    assert!(!text.contains("ALL"), "{text}");
    assert!(
        text.contains("PARTIAL — 1/2 stages ran (skipped: a). Not a green gate"),
        "{text}"
    );
}

#[test]
fn an_unknown_stage_in_only_is_refused_naming_the_known_ones() {
    let e = selection(&["rust:unti".to_string()]).unwrap_err();
    assert!(e.starts_with("unknown stage(s): rust:unti"), "{e}");
    assert!(e.contains("known: shims, "), "{e}");
    assert_eq!(selection(&[]).unwrap(), None);
}

/// The TS gate's 13 stages, frozen: the stage list `scripts/check.ts`
/// held when this gate replaced it.
const TS_STAGES: &[&str] = &[
    "typecheck",
    "lint",
    "rust:fmt",
    "rust:clippy",
    "knip",
    "knip:prod",
    "census:clones",
    "census:concurrency",
    "unit",
    "rust:unit",
    "rust:build",
    "rust:format-golden",
    "e2e",
];

/// docs/dev-harness.md §4 (d): every stage of the old gate has a
/// successor here, or is MOOT with the reason; every successor is a real
/// stage; and the new stages the design adds are present.
#[test]
fn every_ts_gate_stage_maps_to_a_successor() {
    let mapped: Vec<&str> = SUCCESSORS.iter().map(|(old, _)| *old).collect();
    assert_eq!(
        mapped, TS_STAGES,
        "the mapping covers the TS gate, in order"
    );
    for (old, new) in SUCCESSORS {
        if let Some(why) = new.strip_prefix("MOOT: ") {
            assert!(!why.trim().is_empty(), "{old}: MOOT without a reason");
        } else {
            assert!(
                STAGES.iter().any(|s| s.name == *new),
                "{old} -> {new}: not a stage"
            );
        }
    }
    for new in ["no-private-addresses", "census:slop"] {
        assert!(
            STAGES.iter().any(|s| s.name == new),
            "missing new stage {new}"
        );
    }
    let advisory: Vec<&str> = STAGES
        .iter()
        .filter(|s| s.advisory)
        .map(|s| s.name)
        .collect();
    assert_eq!(advisory, ["census:clones", "census:slop"]);
}

#[test]
fn an_in_crate_stage_reruns_this_binary_with_the_user_tools_on_path() {
    let host = HostEnv {
        home: Some("/h".into()),
        path: Some("/bin".into()),
        current_exe: Some("/x/humanify-ag-dev".into()),
        ..HostEnv::default()
    };
    let e2e = STAGES.iter().find(|s| s.name == "e2e").unwrap();
    let s = spawn_of(e2e, Path::new("/repo"), &host).unwrap();
    assert_eq!(s.describe(), "/x/humanify-ag-dev e2e");
    assert_eq!(s.cwd.as_deref(), Some(Path::new("/repo")));
    assert!(
        s.env
            .contains(&("PATH".into(), "/h/.cargo/bin:/h/.bun/bin:/bin".into()))
    );
    let build = STAGES.iter().find(|s| s.name == "rust:build").unwrap();
    assert_eq!(
        spawn_of(build, Path::new("/repo"), &host)
            .unwrap()
            .describe(),
        "cargo build --release --locked -p humanify-ag-cli"
    );
    let no_exe = HostEnv::default();
    assert!(spawn_of(e2e, Path::new("/repo"), &no_exe).is_err());
}
