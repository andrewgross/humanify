//! The living rules of test/measurement-owners.test.ts whose Rust owner
//! exists in the dev crate today (docs/dev-harness.md §6). The rest are
//! either MOOT with the TS/shell they guarded, already L1 tests, or land
//! with the lane writing their owner (the summary, the leaderboard, the
//! eval runner, the scorer's statement extraction) — see the lane L4
//! report for the table.

use std::path::PathBuf;

use humanify_ag_dev::identity::backfill::eval_label;
use humanify_ag_dev::identity::{F, Purpose, comparable};
use humanify_ag_dev::settings;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("owners-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// measurement-owners.test.ts "score refuses to add binary-scored cards to
/// a TS-scored label": a pre-cutover TS label backfills with an UNKNOWN
/// binary ("the TS pipeline: no binary"), and the one guard refuses to add
/// a card whose binary is known — naming each identifying binary field.
#[test]
fn a_binary_card_never_joins_a_ts_label() {
    let dir = scratch("ts-label");
    std::fs::write(dir.join("commit.txt"), "abc1234\n").unwrap();
    std::fs::write(dir.join("pipeline.json"), r#"{"pipeline":{"kind":"ts"}}"#).unwrap();
    std::fs::write(dir.join("2.1.85.json"), r#"{"pair":"2.1.84->2.1.85"}"#).unwrap();
    let ts = eval_label(&dir).unwrap();
    std::fs::remove_dir_all(&dir).unwrap();
    assert_eq!(ts.len(), 1, "the label's one card backfills");
    let ts = &ts[0];
    assert!(
        matches!(&ts.binary.sha256, F::Unknown(why) if why.unknown.contains("TS pipeline")),
        "a TS label's binary is unknown, saying why: {:?}",
        ts.binary.sha256
    );
    let mut binary_card = ts.clone();
    binary_card.binary.path = F::Known("/w/target/release/humanify-ag".into());
    binary_card.binary.sha256 = F::Known("binsha".into());
    binary_card.binary.build_commit = F::Known("abc1234".into());
    binary_card.binary.dirty = F::Known(false);
    binary_card.binary.name = F::Known("humanify-ag".into());
    let refused = comparable(Purpose::AddCard, ts, &binary_card)
        .expect_err("a binary card must not join a TS label");
    let fields: Vec<&str> = refused.iter().map(|m| m.field.as_str()).collect();
    // (The path is a per-card field: a binary is identified by its bytes
    // and build, not by where it sat.)
    for f in [
        "binary.sha256",
        "binary.buildCommit",
        "binary.dirty",
        "binary.name",
    ] {
        assert!(fields.contains(&f), "{f} not named in {fields:?}");
    }
}

/// measurement-owners.test.ts "score accepts the --bin flag family (flags,
/// never env vars)": `--bin` and `--force-mixed` are settings parsed
/// upfront, and the retired preflight flag and the always-on warm
/// self-hop's old switch are refused before anything runs.
#[test]
fn the_bin_flag_family_is_flags_and_retired_switches_fail_upfront() {
    let args = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    let p = settings::parse(
        "eval score",
        &["run"],
        &[],
        &args(&["label", "--bin", "/x/humanify-ag", "--force-mixed"]),
    )
    .unwrap();
    assert_eq!(
        p.settings.text("run.bin").as_deref(),
        Some("/x/humanify-ag")
    );
    for (flag, why) in [
        ("--skip-preflight", "preflight retired"),
        ("--warm-self-hop", "unknown flag --warm-self-hop"),
    ] {
        let refused = settings::parse("eval score", &["run"], &[], &args(&["label", flag]))
            .expect_err(flag)
            .join("\n");
        assert!(refused.contains(why), "{flag}: {refused}");
    }
}
