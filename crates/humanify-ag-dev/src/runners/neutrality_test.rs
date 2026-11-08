//! `eval neutrality` run for real from a sandbox repo whose HEAD is the
//! candidate and whose `baseline` branch is the baseline, both legs
//! "built" by the cargo recorder: every launch must be byte-identical to
//! the exp191 neutrality goldens (proof (c)). The goldens' `run-config|`
//! lines are eval.ts's run config; the Rust run writes the harness's one
//! run config (`settings::run_config`) instead, so those lines are checked
//! by value (`resolved`), not by bytes.

use std::path::PathBuf;

use humanify_ag_model::js::JsValue;

use super::neutrality::{NeutralityConfig, config_from, resolved_record, run};
use crate::runners::recording_test::{Commit, Rec, cargo_toml, committed, golden, sandbox_repo};

const ACCEPTS: &[&str] = &[
    "run.workdir",
    "run.pipeline_set",
    "neutrality",
    "eval.inputs_base",
    "eval.library_index",
    "llm.endpoint",
];

enum Baseline {
    New,
    PreRename,
    Legacy,
}

fn corpus(rec: &Rec) {
    let input = rec
        .root
        .join("inputs/claude-code-2.1.86/binary-decompiled/src/entrypoints/index.js");
    std::fs::create_dir_all(input.parent().unwrap()).unwrap();
    std::fs::write(&input, "input\n").unwrap();
    let prior = rec.root.join("priors/2.1.85-rebased/.humanify");
    std::fs::create_dir_all(&prior).unwrap();
    std::fs::write(prior.join("humanified.js"), "prior\n").unwrap();
}

fn sandbox(rec: &Rec, baseline: Baseline) -> PathBuf {
    let pairs = (
        "experiments/034-eval-harness/pairs.json",
        committed("experiments/034-eval-harness/pairs.json"),
    );
    let new_crate = (
        "crates/humanify-ag-cli/Cargo.toml",
        cargo_toml("humanify-ag-cli", "humanify-ag"),
    );
    let old_crate = (
        "crates/humanify-cli/Cargo.toml",
        cargo_toml("humanify-cli", "humanify"),
    );
    let mut base = vec![pairs.clone()];
    match baseline {
        Baseline::New => base.push(new_crate.clone()),
        Baseline::PreRename => base.push(old_crate),
        Baseline::Legacy => {
            base.push(old_crate);
            base.push(("LEGACY_HELP", String::new()));
        }
    }
    sandbox_repo(
        rec,
        &[
            Commit {
                files: base,
                branch: Some("baseline"),
            },
            Commit {
                files: vec![pairs, new_crate],
                branch: None,
            },
        ],
    )
}

fn args(rec: &Rec, flags: &[&str]) -> Vec<String> {
    let r = rec.root.display().to_string();
    let mut a: Vec<String> = [
        "baseline",
        "2.1.85:2.1.86",
        "--workdir",
        &format!("{r}/work"),
        "--cache",
        &format!("{r}/work/cache"),
        "--priors",
        &format!("{r}/priors"),
        "--inputs-base",
        &format!("{r}/inputs"),
        "--endpoint",
        "http://127.0.0.1:9/v1",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    a.extend(flags.iter().map(|s| s.to_string()));
    a
}

/// The golden's launch lines (its `run-config|` lines and exit line apart),
/// its exit code, and its run config's values by key.
fn golden_parts(name: &str) -> (String, u8, JsValue) {
    let g = golden(&format!("{name}.golden.txt"));
    let launches: Vec<&str> = g
        .lines()
        .filter(|l| !l.starts_with("run-config|") && !l.starts_with("exit "))
        .collect();
    let exit: u8 = g
        .lines()
        .find_map(|l| l.strip_prefix("exit "))
        .unwrap()
        .parse()
        .unwrap();
    let cfg: String = g
        .lines()
        .filter_map(|l| l.strip_prefix("run-config| "))
        .collect::<Vec<_>>()
        .join("\n");
    (
        format!("{}\n", launches.join("\n")),
        exit,
        JsValue::parse(&cfg).unwrap(),
    )
}

fn check(
    name: &str,
    rec: &Rec,
    repo: &std::path::Path,
    cfg: &NeutralityConfig,
    status: u8,
    said: &str,
) {
    let (launches, exit, gcfg) = golden_parts(name);
    let recorded = rec.normalise(&rec.calls(), repo);
    if recorded != launches {
        let path = std::env::temp_dir().join(format!("l3-golden-{name}"));
        let _ = std::fs::write(&path, &recorded);
        panic!(
            "the {name} launches changed (recording at {}):\n{recorded}\n--- golden:\n{launches}\n--- said:\n{said}",
            path.display()
        );
    }
    assert_eq!(status, exit, "{name}: {said}");
    // the run config's values (the golden's are eval.ts's spelling)
    let ours = super::records::obj(resolved_record(cfg));
    for key in [
        "baseline",
        "pair",
        "workdir",
        "cache",
        "priors",
        "inputsBase",
        "namingMode",
        "libraryIndex",
    ] {
        let a = ours
            .as_object()
            .unwrap()
            .get(key)
            .map(|v| rec.normalise(&humanify_ag_model::js::stringify(v), repo));
        let b = gcfg
            .as_object()
            .unwrap()
            .get(key)
            .map(humanify_ag_model::js::stringify);
        assert_eq!(a, b, "{name}: run config {key}");
    }
}

fn run_case(name: &str, baseline: Baseline, flags: &[&str]) {
    let rec = Rec::new(name);
    corpus(&rec);
    let repo = sandbox(&rec, baseline);
    let ctx = rec.ctx(&repo);
    let parsed =
        crate::settings::parse("eval neutrality", ACCEPTS, &[], &args(&rec, flags)).unwrap();
    let pairs = crate::committed::eval::pairs(&repo).unwrap().value;
    let cfg = config_from(&parsed, &pairs).unwrap();
    let rc = crate::settings::run_config("eval neutrality", ACCEPTS, &parsed);
    let status = run(&ctx, &cfg, &pairs, &rc);
    check(name, &rec, &repo, &cfg, status, &ctx.transcript());
}

#[test]
fn names_mode_both_legs_built_at_their_commits_and_launched_alike() {
    run_case("neutrality-names", Baseline::New, &[]);
}

#[test]
fn off_mode_no_index_the_setting_on_both_legs() {
    run_case(
        "neutrality-off",
        Baseline::New,
        &["--pipeline-set", "library.matching=off"],
    );
}

#[test]
fn a_baseline_from_before_the_rename_builds_humanify_cli() {
    run_case("neutrality-baseline-pre-rename", Baseline::PreRename, &[]);
}

#[test]
fn a_legacy_baseline_cannot_take_the_names_index_and_is_refused() {
    run_case("neutrality-legacy-refused", Baseline::Legacy, &[]);
}

/// The legacy leg's own argv: reachable only with no setting at all (no
/// verb flag gets there — off mode is itself a setting), so the config is
/// built directly, as the golden was recorded (neutrality.sh fed a run
/// config with no setting).
#[test]
fn the_legacy_leg_takes_its_settings_as_the_flags_it_had() {
    let name = "neutrality-legacy-leg";
    let rec = Rec::new(name);
    corpus(&rec);
    let repo = sandbox(&rec, Baseline::Legacy);
    let ctx = rec.ctx(&repo);
    let r = rec.root.clone();
    let cfg = NeutralityConfig {
        baseline: "baseline".into(),
        from: "2.1.85".into(),
        to: "2.1.86".into(),
        workdir: r.join("work"),
        cache: r.join("work/cache"),
        priors: r.join("priors"),
        inputs_base: r.join("inputs").display().to_string(),
        endpoint_flag: Some("http://127.0.0.1:9/v1".into()),
        pipeline_sets: Vec::new(),
        naming_mode: "off".into(),
        library_index: None,
    };
    let pairs = crate::committed::eval::pairs(&repo).unwrap().value;
    let status = run(&ctx, &cfg, &pairs, &JsValue::Null);
    check(name, &rec, &repo, &cfg, status, &ctx.transcript());
}

#[test]
fn a_baseline_leg_that_writes_cache_entries_voids_the_verdict() {
    let rec = Rec::new("neutrality-cold");
    corpus(&rec);
    let repo = sandbox(&rec, Baseline::New);
    // the baseline's binary writes one cache entry: the legs asked different questions
    let bin = crate::runners::recording_test::BIN_RECORDER.replace(
        "if [[ -n \"$out\" ]]; then",
        "[[ \"$out\" == *baseline ]] && mkdir -p \"$RECORDER_ROOT/work/cache\" && echo x > \"$RECORDER_ROOT/work/cache/new\"\nif [[ -n \"$out\" ]]; then",
    );
    crate::runners::recording_test::write_exec(&rec.root.join("bin-recorder"), &bin);
    let ctx = rec.ctx(&repo);
    let parsed = crate::settings::parse("eval neutrality", ACCEPTS, &[], &args(&rec, &[])).unwrap();
    let pairs = crate::committed::eval::pairs(&repo).unwrap().value;
    let cfg = config_from(&parsed, &pairs).unwrap();
    let status = run(&ctx, &cfg, &pairs, &JsValue::Null);
    let said = ctx.transcript();
    assert_eq!(status, 1, "{said}");
    assert!(
        said.contains("leg baseline : 1 new entries  <-- MUST be 0"),
        "{said}"
    );
    assert!(said.contains("NOT NEUTRAL"), "{said}");
}
