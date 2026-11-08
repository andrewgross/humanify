//! `eval walk` end to end against the binary recorder: hop N runs with hop
//! N-1's own output as its prior, the run manifests call that prior the
//! walk's previous hop (never the ARCHIVE — the false warning fixed while
//! porting), `--resume` skips finished hops and refuses a changed naming
//! mode, and the report is written with its new section.

use std::path::Path;

use super::walk::{WalkConfig, run, select_versions};
use crate::runners::recording_test::{Commit, Rec, committed, sandbox_repo, write_exec};

const VERSIONS: &[&str] = &["2.1.99", "2.1.100", "2.1.101"];

fn setup() -> (Rec, std::path::PathBuf) {
    let rec = Rec::new("walk");
    for v in VERSIONS {
        let dir = rec.root.join(format!(
            "inputs/claude-code-{v}/binary-decompiled/src/entrypoints"
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("index.js"), "input\n").unwrap();
    }
    // a version with no input is skipped by the range
    std::fs::create_dir_all(rec.root.join("inputs/claude-code-2.1.98")).unwrap();
    write_exec(
        &rec.shims.join("humanify"),
        crate::runners::recording_test::BIN_RECORDER,
    );
    write_exec(&rec.shims.join("bun"), "#!/usr/bin/env bash\nexit 0\n");
    write_exec(&rec.shims.join("npx"), "#!/usr/bin/env bash\nexit 0\n");
    let repo = sandbox_repo(
        &rec,
        &[Commit {
            files: vec![("dev/eval/pairs.json", committed("dev/eval/pairs.json"))],
            branch: None,
        }],
    );
    (rec, repo)
}

fn cfg(rec: &Rec, naming: &str, resume: bool) -> WalkConfig {
    let index = rec.root.join("index");
    std::fs::create_dir_all(&index).unwrap();
    WalkConfig {
        versions_arg: "2.1.98..2.1.101".into(),
        dir: rec.root.join("walk"),
        bin: rec.shims.join("humanify"),
        inputs_base: rec.root.join("inputs").display().to_string(),
        endpoint_flag: Some("http://127.0.0.1:9/v1".into()),
        pipeline_sets: if naming == "off" {
            vec!["library.matching=off".into()]
        } else {
            Vec::new()
        },
        naming_mode: naming.into(),
        library_index: (naming == "names").then(|| index.display().to_string()),
        resume,
        reboot: false,
        boot_prompt: true,
        force_mixed: true,
        report: true,
        boot_model: "claude-opus-4-1".into(),
        heavy_lock: rec.root.join("heavy.lock"),
    }
}

fn read(p: &Path) -> String {
    std::fs::read_to_string(p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

#[test]
fn a_range_selects_the_versions_with_inputs_in_semver_order() {
    let (rec, _) = setup();
    let inputs = rec.root.join("inputs").display().to_string();
    assert_eq!(
        select_versions("2.1.98..2.1.101", &inputs).unwrap(),
        VERSIONS
    );
    assert_eq!(
        select_versions("2.1.100,2.1.99", &inputs).unwrap(),
        ["2.1.100", "2.1.99"]
    );
    assert!(
        select_versions("2.1.98,2.1.99", &inputs).is_err(),
        "a listed version with no input is refused"
    );
}

#[test]
fn each_hop_runs_on_the_previous_hops_own_output() {
    let (rec, repo) = setup();
    let ctx = rec.ctx(&repo);
    let pairs = crate::committed::eval::pairs(&repo).unwrap().value;
    let c = cfg(&rec, "names", false);
    assert_eq!(run(&ctx, &c, &pairs), 0, "{}", ctx.transcript());
    let calls = rec.normalise(&rec.calls(), &repo);
    let launches: Vec<&str> = calls
        .lines()
        .filter(|l| l.contains(" :: <TMP>/inputs/"))
        .collect();
    assert_eq!(launches.len(), 3, "{calls}");
    assert!(
        !launches[0].contains("--prior-version"),
        "the first hop is cold: {}",
        launches[0]
    );
    assert!(
        launches[1].contains("--prior-version <TMP>/walk/trees/2.1.99/.humanify/humanified.js"),
        "{}",
        launches[1]
    );
    assert!(
        launches[2].contains("--prior-version <TMP>/walk/trees/2.1.100/.humanify/humanified.js"),
        "{}",
        launches[2]
    );
    assert!(
        calls.contains("humanify-config| index = \"<TMP>/index\""),
        "names mode hands every hop the index"
    );
    let m = read(&c.dir.join("runs/2.1.100-run.json"));
    assert!(m.contains("\"priorKind\": \"walk-previous-hop\""), "{m}");
    assert!(
        !ctx.transcript().contains("ARCHIVE prior"),
        "{}",
        ctx.transcript()
    );
    let hop = read(&c.dir.join("runs/2.1.101.hop.json"));
    assert!(
        hop.contains("\"priorVersion\": \"2.1.100\"") && hop.contains("\"cold\": false"),
        "{hop}"
    );
    let manifest = read(&c.dir.join("manifest.json"));
    assert!(manifest.contains("\"namingMode\": \"names\""), "{manifest}");
    let report = read(&c.dir.join("report/report.md"));
    for section in [
        "# Version walk report",
        "## Per hop, side by side",
        "## Hops to look at",
        "## Per walk, every column",
        "## Example diffs — rust walk",
    ] {
        assert!(report.contains(section), "{section} missing:\n{report}");
    }
    assert!(c.dir.join("history.git").is_dir());
}

#[test]
fn resume_skips_finished_hops_and_refuses_another_naming_mode() {
    let (rec, repo) = setup();
    let ctx = rec.ctx(&repo);
    let pairs = crate::committed::eval::pairs(&repo).unwrap().value;
    let mut c = cfg(&rec, "names", false);
    c.report = false;
    assert_eq!(run(&ctx, &c, &pairs), 0, "{}", ctx.transcript());
    assert_eq!(
        run(&ctx, &c, &pairs),
        2,
        "a second walk into the dir without --resume is refused"
    );
    let mut again = cfg(&rec, "names", true);
    again.report = false;
    let launches = || {
        rec.calls()
            .lines()
            .filter(|l| l.contains("/inputs/claude-code-"))
            .count()
    };
    let before = launches();
    assert_eq!(run(&ctx, &again, &pairs), 0, "{}", ctx.transcript());
    assert_eq!(
        launches(),
        before,
        "finished hops launch nothing (only the preflight --help runs)"
    );
    let mut off = cfg(&rec, "off", true);
    off.report = false;
    assert_eq!(run(&ctx, &off, &pairs), 2);
    assert!(
        ctx.transcript()
            .contains("--resume refused: .namingMode differs"),
        "{}",
        ctx.transcript()
    );
}
