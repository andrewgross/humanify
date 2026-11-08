//! `eval score` run for real against the run.sh recorder: every launch
//! must be byte-identical to `run-launch.golden.txt` (scratch bases, the
//! default) and `run-launch.seeded-base.golden.txt` — the protocol every
//! binary-scored reference ran with (proof (c)). Ported with them: the
//! behaviours `run-launch.test.ts` held (base modes, refusals, the warm
//! self-hop's two failure modes, `--pipeline-set` at all three sites, the
//! last-scored-pair self-hop, the label record).

use std::path::{Path, PathBuf};

use super::*;
use crate::runners::recording_test::{
    Commit, RUN_RECORDER, Rec, assert_golden, committed, sandbox_repo, write_exec,
};

const SCORE_ACCEPTS: &[&str] = &["run", "eval", "llm", "boot"];
/// The committed pairs (`dev/eval/pairs.json`): the fake corpus holds
/// every one, so the goldens record the full protocol (exp201: a const
/// list here silently SKIPPED the fifth pair and the goldens still passed).
fn pairs() -> Vec<(String, String)> {
    crate::committed::eval::pairs(&crate::paths::repo_root())
        .expect("pairs.json")
        .value
        .pairs
        .into_iter()
        .map(|p| (p.from, p.to))
        .collect()
}

struct Harness {
    status: u8,
    said: String,
    launches: String,
    /// Each scored leg's recorded launch block (launch line + config lines).
    scored: Vec<String>,
    /// Each scored leg's runcfg (normalised).
    runcfgs: Vec<String>,
    results: PathBuf,
    rec: Rec,
    repo: PathBuf,
    label: String,
}

fn fake_corpus(root: &Path) {
    for (from, to) in pairs() {
        for v in [&from, &to] {
            let dir = root.join(format!(
                "inputs/claude-code-{v}/binary-decompiled/src/entrypoints"
            ));
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("index.js"), "input\n").unwrap();
        }
        let prior = root.join(format!("priors/claude-code-{from}/.humanify"));
        std::fs::create_dir_all(&prior).unwrap();
        std::fs::write(prior.join("humanified.js"), "prior\n").unwrap();
        std::fs::write(prior.join("split-ledger.json"), "{}\n").unwrap();
    }
}

fn args(rec: &Rec, label: &str, flags: &[&str]) -> Vec<String> {
    let r = rec.root.display().to_string();
    let mut a: Vec<String> = vec![
        label.into(),
        "--workdir".into(),
        format!("{r}/work"),
        "--inputs-base".into(),
        format!("{r}/inputs"),
        "--priors-base".into(),
        format!("{r}/priors"),
        "--endpoint".into(),
        "http://127.0.0.1:9/v1".into(),
    ];
    a.extend(flags.iter().map(|f| f.replace("<TMP>", &r)));
    a
}

/// A launch block: its launch line and the config lines under it.
fn blocks(calls: &str) -> Vec<Vec<String>> {
    let mut out: Vec<Vec<String>> = Vec::new();
    for line in calls.lines() {
        if line.starts_with("humanify-config|") {
            if let Some(b) = out.last_mut() {
                b.push(line.to_string());
            }
        } else {
            out.push(vec![line.to_string()]);
        }
    }
    out
}

/// The scored leg's block: its config file is `<work>/<label>/<v>.toml`.
fn is_scored(launch: &str) -> bool {
    launch.starts_with("humanify ")
        && pairs()
            .iter()
            .any(|(_, to)| launch.contains(&format!("/<LABEL>/{to}.toml ")))
}

fn run_score(flags: &[&str], prep: impl Fn(&Rec)) -> Result<Harness, Vec<String>> {
    let rec = Rec::new("score");
    write_exec(&rec.shims.join("humanify"), RUN_RECORDER);
    write_exec(&rec.shims.join("npx"), RUN_RECORDER);
    write_exec(&rec.shims.join("bun"), "#!/usr/bin/env bash\nexit 0\n");
    write_exec(
        &rec.home.join(".cargo/bin/cargo"),
        "#!/usr/bin/env bash\necho \"cargo :: $* (in $PWD)\" >> \"$RECORDER_ROOT/calls.log\"\nexit 1\n",
    );
    fake_corpus(&rec.root);
    let repo = sandbox_repo(
        &rec,
        &[Commit {
            files: vec![
                ("dev/eval/pairs.json", committed("dev/eval/pairs.json")),
                (
                    "dev/eval/self-hop-reference.json",
                    committed("dev/eval/self-hop-reference.json"),
                ),
            ],
            branch: None,
        }],
    );
    let label = format!(
        "__run-launch-test-{}",
        rec.root.file_name().unwrap().to_string_lossy()
    );
    let results = repo.join(crate::paths::EVAL_RESULTS).join(&label);
    for (_, to) in pairs() {
        // the scored leg's stats artifact (run-pipeline.ts's recorder wrote it)
        std::fs::create_dir_all(&results).unwrap();
        std::fs::write(results.join(format!("{to}.stats.json")), "{}\n").unwrap();
    }
    prep(&rec);
    let ctx = rec.ctx(&repo);
    let parsed =
        crate::settings::parse("eval score", SCORE_ACCEPTS, &[], &args(&rec, &label, flags))?;
    let pairs = crate::committed::eval::pairs(&repo).unwrap().value;
    let cfg = score::config_from(&parsed, &pairs, &ctx).map_err(|e| vec![e])?;
    let status = score::run(
        &ctx,
        &cfg,
        &crate::settings::run_config("eval score", SCORE_ACCEPTS, &parsed),
    );
    let norm = |t: &str| rec.normalise(t, &repo).replace(&label, "<LABEL>");
    let calls = norm(&rec.calls());
    let mut launches: Vec<String> = Vec::new();
    let mut scored = Vec::new();
    for b in blocks(&calls) {
        let head = &b[0];
        let keep = head.starts_with("humanify ") || head.starts_with("cargo ");
        if !keep {
            continue;
        }
        if is_scored(head) {
            scored.push(b.join("\n"));
        } else {
            launches.extend(b);
        }
    }
    let work = rec.root.join("work").join(&label);
    let mut runcfgs: Vec<(String, String)> = std::fs::read_dir(&work)
        .map(|d| {
            d.filter_map(|e| e.ok()?.file_name().into_string().ok())
                .filter_map(|n| n.strip_suffix(".runcfg.json").map(str::to_string))
                .map(|v| {
                    let text =
                        std::fs::read_to_string(work.join(format!("{v}.runcfg.json"))).unwrap();
                    (format!("runcfg-{v}.json"), norm(&text))
                })
                .collect()
        })
        .unwrap_or_default();
    runcfgs.sort();
    let mut all = launches.clone();
    all.extend(runcfgs.iter().map(|(n, t)| format!("{n}: {t}")));
    Ok(Harness {
        status,
        said: norm(&ctx.transcript()),
        launches: if calls.is_empty() {
            String::new()
        } else {
            all.join("\n")
        },
        scored,
        runcfgs: runcfgs.into_iter().map(|(_, t)| t).collect(),
        results,
        rec,
        repo,
        label,
    })
}

fn bin_flags() -> Vec<&'static str> {
    vec!["--bin", "<TMP>/shims/humanify", "--force-mixed"]
}

fn with(extra: &[&'static str]) -> Vec<&'static str> {
    let mut f = bin_flags();
    f.extend_from_slice(extra);
    f
}

/// Every scored leg launched exactly its runcfg: the argv and the config
/// file it records.
fn assert_scored_launches_match_runcfgs(h: &Harness) {
    assert_eq!(h.scored.len(), h.runcfgs.len(), "{}", h.said);
    for cfg in &h.runcfgs {
        let v = humanify_ag_model::js::JsValue::parse(cfg).unwrap();
        let o = v.as_object().unwrap();
        let args: Vec<String> = match o.get("args").unwrap() {
            humanify_ag_model::js::JsValue::Array(a) => {
                a.iter().map(|x| x.as_str().unwrap().to_string()).collect()
            }
            _ => panic!("args"),
        };
        let toml = o.get("configToml").unwrap().as_str().unwrap();
        let mut want = vec![format!(
            "humanify NODE_OPTIONS= HUMANIFY_API_KEY=local :: {} ",
            args.join(" ")
        )];
        want.extend(toml.lines().map(|l| format!("humanify-config| {l}")));
        let want = want.join("\n");
        assert!(
            h.scored.contains(&want),
            "no scored launch is its runcfg:\n{want}\n--- launched:\n{}",
            h.scored.join("\n")
        );
    }
}

#[test]
fn every_launch_is_byte_identical_to_the_scratch_golden() {
    let h = run_score(&bin_flags(), |_| {}).unwrap();
    assert_ran(&h);
    assert_golden("run-launch.golden.txt", &h.launches);
    assert_scored_launches_match_runcfgs(&h);
}

#[test]
fn seeded_base_launches_are_byte_identical_to_the_seeded_golden() {
    let h = run_score(&with(&["--base-mode", "seeded"]), |_| {}).unwrap();
    assert_ran(&h);
    assert_golden("run-launch.seeded-base.golden.txt", &h.launches);
    assert_scored_launches_match_runcfgs(&h);
}

#[test]
fn archive_mode_rebuilds_nothing() {
    let h = run_score(&with(&["--base-mode", "archive"]), |_| {}).unwrap();
    assert_ran(&h);
    assert!(!h.launches.contains("-rebased "), "{}", h.launches);
    assert_eq!(
        h.launches.matches("\"baseMode\": \"archive\"").count(),
        pairs().len()
    );
    let p = std::fs::read_to_string(h.results.join("pipeline.json")).unwrap();
    assert!(p.contains("\"baseMode\":\"archive\""), "{p}");
}

#[test]
fn the_label_records_the_binary_the_run_config_and_the_boot_model() {
    let h = run_score(&bin_flags(), |_| {}).unwrap();
    let p = std::fs::read_to_string(h.results.join("pipeline.json")).unwrap();
    assert!(
        p.starts_with("{\"pipeline\":{\"kind\":\"rust-bin\",\"bin\":{"),
        "{p}"
    );
    assert!(p.contains("\"bootGateModel\":\"claude-opus-4-1\""), "{p}");
    let cfg = h
        .rec
        .root
        .join("work")
        .join(format!("{}.run-config.json", h.label));
    let sha = crate::committed::sha256_hex(&std::fs::read(cfg).unwrap());
    assert!(p.contains(&format!("\"sha256\":\"{sha}\"")), "{p}");
    assert!(
        std::fs::read_to_string(h.results.join("commit.txt"))
            .unwrap()
            .trim()
            .len()
            >= 7
    );
    assert!(h.results.join("2.1.86-run-status.json").is_file());
    assert!(h.repo.is_dir());
}

#[test]
fn a_binary_of_an_unknown_commit_is_refused_without_force_mixed() {
    let h = run_score(&["--bin", "<TMP>/shims/humanify"], |_| {}).unwrap();
    assert_eq!(h.status, 2, "{}", h.said);
    assert!(h.said.contains("--force-mixed"), "{}", h.said);
    assert!(
        !h.launches.contains("humanify "),
        "nothing may launch after a refusal"
    );
}

#[test]
fn without_bin_the_harness_builds_the_repo_binary_and_stops_when_the_build_fails() {
    let h = run_score(&[], |_| {}).unwrap();
    assert_eq!(h.status, 1, "{}", h.said);
    assert!(
        h.said
            .contains("building and recording <REPO>/target/release/humanify-ag"),
        "{}",
        h.said
    );
    // the sandbox repo holds no pipeline crate, so the build is refused
    // before cargo; the real checkout's build is `ctx.build` (bin_build).
    assert!(h.said.contains("could not build/record"), "{}", h.said);
    assert!(!h.launches.contains("humanify NODE"), "{}", h.launches);
}

#[test]
fn refusals_happen_before_anything_launches() {
    for bad in [
        &["--pipeline-set", "nodot=1"][..],
        &["--pipeline-set", "llm.concurrency=8"],
        &["--pipeline-arg", "--sequential"],
        &["--no-vendor"],
        &["--heap-mb", "1"],
        &["--seeded-base"],
    ] {
        let r = run_score(&with(bad), |_| {});
        match r {
            Err(_) => {}
            Ok(h) => panic!("{bad:?} was not refused: {}", h.said),
        }
    }
}

#[test]
fn pipeline_sets_reach_all_three_sites_once_each_in_order() {
    let h = run_score(
        &with(&[
            "--pipeline-set",
            "naming.schedule=sequential",
            "--pipeline-set",
            "naming.batch_size=10",
        ]),
        |_| {},
    )
    .unwrap();
    assert_ran(&h);
    let want = "humanify-config| [naming]\nhumanify-config| schedule = \"sequential\"\nhumanify-config| batch_size = 10";
    // Every scored leg plus the two self-hop legs (cold and warm).
    let n = pairs().len();
    assert_eq!(h.launches.matches(want).count(), n + 2, "{}", h.launches);
    assert_eq!(h.scored.iter().filter(|b| b.contains(want)).count(), n);
    let p = std::fs::read_to_string(h.results.join("pipeline.json")).unwrap();
    assert!(
        p.contains(
            "\"pipelineSettings\":[\"naming.schedule=sequential\",\"naming.batch_size=10\"]"
        ),
        "{p}"
    );
}

fn self_hop(h: &Harness) -> humanify_ag_model::js::JsValue {
    let f = std::fs::read_dir(&h.results)
        .unwrap()
        .filter_map(|e| e.ok())
        .find(|e| e.file_name().to_string_lossy().ends_with("-self-hop.json"))
        .unwrap_or_else(|| panic!("no self-hop verdict:\n{}", h.said));
    let v =
        humanify_ag_model::js::JsValue::parse(&std::fs::read_to_string(f.path()).unwrap()).unwrap();
    v.as_object().unwrap().get("selfHop").unwrap().clone()
}

fn get<'a>(
    v: &'a humanify_ag_model::js::JsValue,
    path: &[&str],
) -> &'a humanify_ag_model::js::JsValue {
    let mut c = v;
    for k in path {
        c = c.as_object().unwrap().get(k).unwrap();
    }
    c
}

#[test]
fn the_warm_self_hop_replays_a_copy_identical_with_zero_writes() {
    let h = run_score(&bin_flags(), |_| {}).unwrap();
    let hop = self_hop(&h);
    use humanify_ag_model::js::JsValue;
    assert_eq!(get(&hop, &["warm", "ran"]), &JsValue::Bool(true));
    assert_eq!(get(&hop, &["warm", "identical"]), &JsValue::Bool(true));
    assert_eq!(get(&hop, &["warm", "cacheWrites"]), &JsValue::Number(0.0));
    assert_eq!(get(&hop, &["warm", "ok"]), &JsValue::Bool(true));
}

#[test]
fn the_warm_self_hop_fails_on_a_cache_write_and_on_a_byte_difference() {
    // The recorder reads RECORDER_CACHE_WRITE / RECORDER_DIVERGE from its
    // environment: wrap it so the variable reaches it.
    for (key, value) in [
        ("RECORDER_CACHE_WRITE", "selfhop-warm-cache"),
        ("RECORDER_DIVERGE", "selfhop-warm"),
    ] {
        let h = run_score(&bin_flags(), |rec| {
            let wrapped = format!(
                "#!/usr/bin/env bash\nexport {key}=\"{value}\"\nexec \"{}\" \"$@\"\n",
                rec.root.join("real-humanify").display()
            );
            write_exec(&rec.root.join("real-humanify"), RUN_RECORDER);
            write_exec(&rec.shims.join("humanify"), &wrapped);
        })
        .unwrap();
        let hop = self_hop(&h);
        use humanify_ag_model::js::JsValue;
        assert_eq!(
            get(&hop, &["warm", "ok"]),
            &JsValue::Bool(false),
            "{key}: {}",
            h.said
        );
        assert!(h.said.contains("WARM SELF-HOP FAILED"), "{}", h.said);
    }
}

#[test]
fn a_pairs_subset_self_hops_the_last_scored_pair() {
    let h = run_score(&with(&["--pairs", "85->86"]), |_| {}).unwrap();
    assert_ran(&h);
    use humanify_ag_model::js::JsValue;
    assert_eq!(get(&self_hop(&h), &["version"]), &JsValue::str("2.1.86"));
}

#[test]
fn a_missing_diagnostics_trail_is_said_out_loud() {
    let h = run_score(&bin_flags(), |_| {}).unwrap();
    assert!(h.said.contains("NO DIAGNOSTICS TRAIL"), "{}", h.said);
}

/// Plant a label's earlier record before the run.
fn plant(rec: &Rec, pipeline: Option<&str>, manifest: Option<&str>) {
    let repo = rec.root.join("repo");
    let head = crate::proc::git(&repo, &["rev-parse", "--short", "HEAD"]).unwrap();
    let results = std::fs::read_dir(repo.join(crate::paths::EVAL_RESULTS))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    std::fs::write(results.join("commit.txt"), format!("{head}\n")).unwrap();
    if let Some(p) = pipeline {
        std::fs::write(results.join("pipeline.json"), p).unwrap();
    }
    if let Some(m) = manifest {
        std::fs::write(results.join("2.1.86-run.json"), m).unwrap();
    }
}

#[test]
fn a_scratch_run_into_a_seeded_label_is_refused_and_a_legacy_label_reads_its_manifests() {
    let h = run_score(&bin_flags_strict(), |rec| plant(rec, Some("{\"pipeline\":{\"kind\":\"rust-bin\"},\"baseMode\":\"seeded\",\"namingMode\":\"names\",\"libraryIndex\":\"/work/library-index/cb64c1f5fd507bee7f80efd46e0d01cec405a8b3028a9c05d56e94d7c071ec28\"}\n"), None)).unwrap();
    assert_eq!(h.status, 2, "{}", h.said);
    assert!(
        h.said
            .contains("base modes: the label holds seeded, this run would add scratch"),
        "{}",
        h.said
    );
    // no baseMode in pipeline.json: a rebased prior in the manifests WAS seeded
    let legacy = "{\"pair\":\"2.1.85->2.1.86\",\"inputs\":{\"prior\":\"/w/x/2.1.85-rebased/.humanify/humanified.js\",\"priorKind\":\"rebased\"}}";
    let h = run_score(&bin_flags_strict(), |rec| {
        plant(rec, Some("{\"pipeline\":{\"kind\":\"rust-bin\"},\"namingMode\":\"names\",\"libraryIndex\":\"/work/library-index/cb64c1f5fd507bee7f80efd46e0d01cec405a8b3028a9c05d56e94d7c071ec28\"}\n"), Some(legacy))
    })
    .unwrap();
    assert_eq!(h.status, 2, "{}", h.said);
    assert!(h.said.contains("the label holds seeded"), "{}", h.said);
    // and --force-mixed proceeds, saying so
    let mut forced = bin_flags_strict();
    forced.push("--force-mixed");
    let h = run_score(&forced, |rec| {
        plant(
            rec,
            Some("{\"pipeline\":{\"kind\":\"rust-bin\"},\"baseMode\":\"seeded\"}\n"),
            None,
        )
    })
    .unwrap();
    assert!(h.said.contains("!! MIXED BASE MODES"), "{}", h.said);
}

#[test]
fn a_run_with_another_naming_mode_or_index_into_a_label_is_refused() {
    let h = run_score(&with_strict(&["--pipeline-set", "library.matching=off"]), |rec| {
        plant(rec, Some("{\"pipeline\":{\"kind\":\"rust-bin\"},\"baseMode\":\"scratch\",\"namingMode\":\"names\",\"libraryIndex\":\"/ix\"}\n"), None)
    })
    .unwrap();
    assert_eq!(h.status, 2, "{}", h.said);
    assert!(
        h.said
            .contains("naming modes: the label holds names, this run would add off"),
        "{}",
        h.said
    );
    assert!(
        h.said
            .contains("library indexes: the label holds /ix, this run would add null"),
        "{}",
        h.said
    );
}

/// `--bin` without `--force-mixed` (the label guards are under test; the
/// recorder binary's commit is unknown, so the guards must refuse first).
fn bin_flags_strict() -> Vec<&'static str> {
    vec!["--bin", "<TMP>/shims/humanify"]
}

fn with_strict(extra: &[&'static str]) -> Vec<&'static str> {
    let mut f = bin_flags_strict();
    f.extend_from_slice(extra);
    f
}

/// The run went to the end. The recorder's trees are not bundles, so the
/// scorer (scorer v2, lane L2a) writes no card and the summary fails with
/// exit 1 — the one failure a recorder run is allowed.
fn assert_ran(h: &Harness) {
    assert!(h.said.contains("=== summarizing model"), "{}", h.said);
    assert!(
        h.status == 0 || (h.status == 1 && h.said.contains("SUMMARY FAILED: no scorecards")),
        "{}",
        h.said
    );
}

/// Two runs of one label must not write one work folder: a run whose
/// `<workdir>/<label>` another live run holds is refused before it builds
/// or launches anything.
#[test]
fn a_label_folder_another_live_run_writes_is_refused() {
    let held = std::cell::RefCell::new(None);
    let h = run_score(&bin_flags(), |rec| {
        let label = format!(
            "__run-launch-test-{}",
            rec.root.file_name().unwrap().to_string_lossy()
        );
        let folder = rec.root.join("work").join(label);
        *held.borrow_mut() = Some(
            crate::runners::claim::claim_folder(&folder, "eval score (the other run)").unwrap(),
        );
    })
    .unwrap();
    assert_eq!(h.status, 2, "{}", h.said);
    assert!(h.said.contains("eval score (the other run)"), "{}", h.said);
    assert!(h.launches.is_empty(), "nothing launched: {}", h.launches);
}
