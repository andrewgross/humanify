//! The library runner's launches — the build, each `discover` under
//! `taskset` and its config file — against the exp191 library goldens. The
//! goldens' `python ::` lines are `corpus.py verify` and `score.py`: the
//! corpus and the scoring are the library scorer's (lane L2b), so they are
//! left out of the comparison; every other line is compared byte for byte.

use std::path::Path;

use super::library::{DiscoverRun, Entry, build, run_discover, write_rule_config};
use crate::runners::recording_test::{Commit, Rec, cargo_toml, golden, sandbox_repo, write_exec};

fn entries(root: &Path, which: &[&str]) -> Vec<Entry> {
    [("app-big", "webpack", 2000u64), ("app-small", "bun", 10)]
        .iter()
        .filter(|(id, _, _)| which.contains(id))
        .map(|(id, bundler, bytes)| {
            let bundle = root.join(format!("data/corpus/{id}/bundle.js"));
            std::fs::create_dir_all(bundle.parent().unwrap()).unwrap();
            std::fs::write(&bundle, "x\n").unwrap();
            Entry {
                id: id.to_string(),
                bundle: bundle.display().to_string(),
                bundler: bundler.to_string(),
                bytes: *bytes,
            }
        })
        .collect()
}

fn run_case(name: &str, rule: &str, cpus: &str, sets: &[&str], which: &[&str]) {
    let rec = Rec::new(name);
    write_exec(
        &rec.shims.join("taskset"),
        "#!/bin/bash\necho \"taskset :: $*\" >> \"$RECORDER_ROOT/calls.log\"\nshift 2\nexec \"$@\"\n",
    );
    let repo = sandbox_repo(
        &rec,
        &[Commit {
            files: vec![(
                "crates/humanify-ag-cli/Cargo.toml",
                cargo_toml("humanify-ag-cli", "humanify-ag"),
            )],
            branch: None,
        }],
    );
    let ctx = rec.ctx(&repo);
    let bin = build(&ctx, None, &rec.root.join("builds")).unwrap();
    let run_dir = rec.root.join("data/results/lbl");
    let settings: Vec<String> = sets.iter().map(|s| s.to_string()).collect();
    let config = write_rule_config(&run_dir, rule, &settings).unwrap();
    let index = rec.root.join("index");
    let reports = run_dir.join("reports");
    let run = DiscoverRun {
        bin: &bin.path,
        index: &index,
        config: &config,
        reports: &reports,
        cpus,
        jobs: 1,
        cwd: Some(rec.root.clone()),
    };
    run_discover(&ctx, &run, &entries(&rec.root, which)).unwrap();
    let recorded = rec.normalise(&rec.calls(), &repo);
    let want: String = golden(&format!("{name}.golden.txt"))
        .lines()
        .filter(|l| !l.starts_with("python :: "))
        .map(|l| format!("{l}\n"))
        .collect();
    assert_eq!(recorded, want, "{}", ctx.transcript());
    for id in which {
        let rj = std::fs::read_to_string(reports.join(format!("{id}.run.json"))).unwrap();
        assert!(rj.starts_with("{\n \"peakRssMb\": "), "{rj}");
    }
}

#[test]
fn structural_rule_report_mode_biggest_bundle_first() {
    run_case(
        "library-structural",
        "structural",
        "0-15",
        &[],
        &["app-big", "app-small"],
    );
}

#[test]
fn agreement_rule_with_a_set_and_a_pipeline_setting() {
    run_case(
        "library-agreement-sets",
        "agreement",
        "16-31",
        &["library.matching=names"],
        &["app-big"],
    );
}
