//! `humanify-ag-dev` (`cargo dev <verb>`): THE verb table of the
//! measurement harness (docs/dev-harness.md §2.4). Every instrument is one
//! entry with its usage, what it PROVES and what it CANNOT prove (a test
//! fails an entry without both, as measurement-owners.test.ts did for
//! eval.ts), the settings it reads, and — until its lane lands — the lane
//! that ports it. A verb not yet ported says so and exits non-zero; it
//! never half-runs. `cargo dev` with no arguments prints the table.

use std::process::ExitCode;

use humanify_ag_dev::{bin_build, env, settings};

/// One verb.
struct Verb {
    /// The words that name it (`["eval", "score"]`).
    words: &'static [&'static str],
    usage: &'static str,
    proves: &'static str,
    cannot_prove: &'static str,
    /// None: implemented. Some(lane): not yet ported; the lane that ports it.
    lane: Option<&'static str>,
    /// What runs today until the lane lands.
    today: &'static str,
    /// The settings it reads: harness keys or whole sections.
    settings: &'static [&'static str],
}

const SCORE_SETTINGS: &[&str] = &["run", "eval", "llm", "boot"];
const NEUTRALITY_SETTINGS: &[&str] = &[
    "run.workdir",
    "run.pipeline_set",
    "neutrality",
    "eval.inputs_base",
    "eval.library_index",
    "llm.endpoint",
];
const LIBRARY_SETTINGS: &[&str] = &["run.force_mixed", "run.pipeline_set", "libscore"];
const FORCE: &[&str] = &["run.force_mixed"];
const INDEX_SECTION: &str = "index";
const INDEX_SETTINGS: &[&str] = &[INDEX_SECTION];
const NONE: &[&str] = &[];

const VERBS: &[Verb] = &[
    Verb {
        words: &["check"],
        usage: "check [--only a,b]",
        proves: "every gate stage passed on this tree (PARTIAL when --only)",
        cannot_prove: "anything about LLM-dependent output: the gate runs the pipeline against a stub",
        lane: Some("L4"),
        today: "npm run check",
        settings: NONE,
    },
    Verb {
        words: &["eval", "score"],
        usage: "eval score <label> [--pairs a->b,...] [--base-mode scratch|seeded|archive] [--llm-cache D] [--force-mixed] [--bin PATH] [--pipeline-set section.key=value]...",
        proves: "how the CURRENT TREE's cross-version diff decomposes (KPIs), pipeline exit, boot",
        cannot_prove: "any delta inside the measured noise-bands.json floor — it will still print a sign",
        lane: Some("L3"),
        today: "npm run eval -- score",
        settings: SCORE_SETTINGS,
    },
    Verb {
        words: &["eval", "rescore"],
        usage: "eval rescore <label> --work <dir>",
        proves: "what the current scorer reads off a label's recorded trees",
        cannot_prove: "anything about the pipeline: no binary runs",
        lane: Some("L2a"),
        today: "npx tsx experiments/170-app-vendor-split/rescore.ts",
        settings: NONE,
    },
    Verb {
        words: &["eval", "summarize"],
        usage: "eval summarize <label>",
        proves: "nothing new — a re-render of recorded cards and verdicts",
        cannot_prove: "validity of cards recorded without run-status (UNKNOWN, not clean)",
        lane: Some("L2a"),
        today: "npm run eval -- summarize",
        settings: NONE,
    },
    Verb {
        words: &["eval", "leaderboard"],
        usage: "eval leaderboard <label> [label...] [--force-mixed]",
        proves: "relative KPI movement between labels whose identities the guard accepts",
        cannot_prove: "that a sub-noise-floor delta is real; only novel/realLn have proven draw-invariance",
        lane: Some("L2a"),
        today: "npm run eval -- leaderboard",
        settings: FORCE,
    },
    Verb {
        words: &["eval", "bands"],
        usage: "eval bands <label> <label> [label...]",
        proves: "how much two runs of IDENTICAL code disagree per KPI — the floor a delta must clear",
        cannot_prove: "anything from labels at different commits — that measures a change, not a floor (refused)",
        lane: Some("L2a"),
        today: "npm run eval -- bands",
        settings: NONE,
    },
    Verb {
        words: &["eval", "diff"],
        usage: "eval diff <priorTree> <freshTree>",
        proves: "what the on-disk diff between THESE two trees is made of, git-capped",
        cannot_prove: "which mechanism caused a line",
        lane: Some("L2a"),
        today: "npm run eval -- diff",
        settings: NONE,
    },
    Verb {
        words: &["eval", "neutrality"],
        usage: "eval neutrality <baseline-ref> [from:to] [--workdir D] [--cache D] [--priors D] [--inputs-base D] [--endpoint URL] [--library-index D] [--pipeline-set section.key=value]...",
        proves: "a refactor changed NOTHING: 0 differing files/lines, baseline leg wrote 0 cache entries",
        cannot_prove: "anything from a COLD run (baseline leg wrote entries) — cold verdicts are void, null-control proven 2026-08-11",
        lane: Some("L3"),
        today: "npm run eval -- neutrality",
        settings: NEUTRALITY_SETTINGS,
    },
    Verb {
        words: &["eval", "vendor-precision"],
        usage: "eval vendor-precision check <diag.json> --truth bun-dev:<id>|cc216|none  |  eval vendor-precision bun-dev <bin> <label> [jobs]",
        proves: "app/wrong-package bytes among the truth-LABELLED vendor bytes, through the run's own raw-to-era bridge",
        cannot_prove: "anything about unlabelled bytes, or a run without the bridge (pre-exp166 binaries)",
        lane: Some("L2b"),
        today: "npm run eval -- vendor-precision",
        settings: NONE,
    },
    Verb {
        words: &["eval", "cjs-placement"],
        usage: "eval cjs-placement <label>",
        proves: "where a CommonJS app's modules were placed against the bun-dev census",
        cannot_prove: "anything outside the census's bundles",
        lane: Some("L2b"),
        today: "python3 experiments/172-cjs-app-placement/score.py",
        settings: NONE,
    },
    Verb {
        words: &["eval", "walk"],
        usage: "eval walk <from> <to>",
        proves: "how the diff behaves hop after hop on the pipeline's OWN previous output",
        cannot_prove: "the eval's KPIs: a walk diff is not a scored card",
        lane: Some("L3"),
        today: "experiments/walk/walk.sh",
        settings: SCORE_SETTINGS,
    },
    Verb {
        words: &["run", "stub"],
        usage: "run stub <bin> <bundle> <out> [--pipeline-set section.key=value]...",
        proves: "what the binary does on a bundle with every name answered by the one stub (no LLM)",
        cannot_prove: "anything about LLM naming quality",
        lane: Some("L3"),
        today: "npx tsx experiments/166-restore-or-ask/stub-run.mts",
        settings: &["run.pipeline_set"],
    },
    Verb {
        words: &["library", "score"],
        usage: "library score <label> --index <dir> --rule agreement|structural [--commit REF] [--jobs N] [--cpus LIST] [--corpus NAME] [--sets a,b] [--force-mixed] [--pipeline-set library.<key>=value]...",
        proves: "precision / recall / F0.5 of library naming on a frozen corpus, and that no app code was claimed",
        cannot_prove: "anything about a bundle outside the corpus",
        lane: Some("L2b"),
        today: "npm run eval -- library",
        settings: LIBRARY_SETTINGS,
    },
    Verb {
        words: &["library", "leaderboard"],
        usage: "library leaderboard <label> <label> [label...] [--force-mixed]",
        proves: "relative library-matching movement between labels on the fixed corpus (band 0: every delta is real)",
        cannot_prove: "why a delta happened when libraryBytes moved too",
        lane: Some("L2b"),
        today: "npm run eval -- library-leaderboard",
        settings: FORCE,
    },
    Verb {
        words: &["library", "determinism"],
        usage: "library determinism <a> <b>",
        proves: "two labels of one configuration are byte-identical (the band-0 claim)",
        cannot_prove: "anything about two different configurations",
        lane: Some("L2b"),
        today: "python3 experiments/096-library-matching-score/determinism.py",
        settings: NONE,
    },
    Verb {
        words: &["library", "corpus"],
        usage: "library corpus build|freeze|verify <corpus>",
        proves: "a corpus's frozen files are byte-identical to its manifest",
        cannot_prove: "that the truth labels are correct",
        lane: Some("L2b"),
        today: "python3 experiments/096-library-matching-score/corpus.py",
        settings: NONE,
    },
    Verb {
        words: &["library", "layers"],
        usage: "library layers <corpus>",
        proves: "re-freezing a corpus's truth layers reproduces every committed layer sha",
        cannot_prove: "that a layer's rule is the right rule",
        lane: Some("L2b"),
        today: "experiments/150-bun-testbed/scripts/layers.sh",
        settings: NONE,
    },
    Verb {
        words: &["index", "build"],
        usage: "index build <spec>",
        proves: "a library index built from a committed spec, chunked, with one merge",
        cannot_prove: "that the index's packages are the right ones",
        lane: Some("L3"),
        today: "scripts/build-index.sh (branch exp184-index-spec)",
        settings: INDEX_SETTINGS,
    },
    Verb {
        words: &["boot-gate"],
        usage: "boot-gate <tree> <version>",
        proves: "a humanified Claude Code tree boots (--version) and answers a live prompt",
        cannot_prove: "behaviour beyond the boot and one prompt",
        lane: Some("L3"),
        today: "experiments/lib/boot-gate.sh",
        settings: &["boot"],
    },
    Verb {
        words: &["build-bin"],
        usage: "build-bin [<checkout-dir>]",
        proves: "which binary a checkout builds: its path, sha256, build commit and dirt (the package and binary name read from THAT checkout)",
        cannot_prove: "anything about what the binary does",
        lane: None,
        today: "",
        settings: NONE,
    },
    Verb {
        words: &["e2e"],
        usage: "e2e",
        proves: "the release binary on every committed fixture: fresh, prior, deterministic twice, and the exact export surface",
        cannot_prove: "LLM naming quality: the names come from a stub",
        lane: Some("L4"),
        today: "npm run test:e2e",
        settings: NONE,
    },
    Verb {
        words: &["format-golden"],
        usage: "format-golden",
        proves: "the formatter's bytes equal the frozen goldens, and a planted perturbation is caught",
        cannot_prove: "formatting of inputs outside the goldens",
        lane: Some("L4"),
        today: "scripts/format-golden.sh",
        settings: NONE,
    },
    Verb {
        words: &["census", "clones"],
        usage: "census clones [--loose]",
        proves: "where twin functions sit across crates (ADVISORY)",
        cannot_prove: "that two twins answer the same question the same way",
        lane: Some("L4"),
        today: "npm run census:clones",
        settings: NONE,
    },
    Verb {
        words: &["census", "concurrency"],
        usage: "census concurrency [--files a.rs,...]",
        proves: "every concurrency site in non-test crates/ is confirmed on the allowlist with its pattern",
        cannot_prove: "that a confirmed site actually follows its pattern's rules",
        lane: Some("L4"),
        today: "npm run census:concurrency",
        settings: NONE,
    },
    Verb {
        words: &["census", "slop"],
        usage: "census slop",
        proves: "per-crate complexity mass, erosion and clone share against main (ADVISORY)",
        cannot_prove: "correctness: a measure of style",
        lane: Some("L4"),
        today: "(new)",
        settings: NONE,
    },
    Verb {
        words: &["ask-trace"],
        usage: "ask-trace [--prior] [--sequential] [--collide NAME] [--set section.key=value]...",
        proves: "every ask the binary makes on a fixture, against the stub",
        cannot_prove: "what a real model would answer",
        lane: Some("L4"),
        today: "npm run ask-trace",
        settings: NONE,
    },
    Verb {
        words: &["diff-asks"],
        usage: "diff-asks <a> <b>",
        proves: "how two ask dumps differ",
        cannot_prove: "why they differ",
        lane: Some("L4"),
        today: "npm run diff-asks",
        settings: NONE,
    },
    Verb {
        words: &["match-truth"],
        usage: "match-truth",
        proves: "the matcher's agreement with ground truth on a pinned npm corpus",
        cannot_prove: "matching on bundles outside the corpus",
        lane: Some("L4"),
        today: "npm run match-truth",
        settings: NONE,
    },
    Verb {
        words: &["env-check"],
        usage: "env-check",
        proves: "the toolchain pin, clippy and rustfmt are present and the MSRV equals the pin",
        cannot_prove: "anything about the code",
        lane: Some("L4"),
        today: "scripts/rust-env-check.sh",
        settings: NONE,
    },
    Verb {
        words: &["precommit"],
        usage: "precommit",
        proves: "staged files are formatted and pass the staged-file censuses",
        cannot_prove: "that the gate passes: the hook is not the gate",
        lane: Some("L4"),
        today: ".husky/pre-commit (lint-staged)",
        settings: NONE,
    },
    Verb {
        words: &["stub-llm", "serve"],
        usage: "stub-llm serve [--port N]",
        proves: "nothing: it serves the one deterministic stub LLM for by-hand runs",
        cannot_prove: "anything about a real model",
        lane: Some("L4"),
        today: "scripts/lib/stub-llm.ts",
        settings: NONE,
    },
];

fn name(v: &Verb) -> String {
    v.words.join(" ")
}

/// The verb `args` names (the longest match) and its own arguments.
fn find(args: &[String]) -> Option<(&'static Verb, &[String])> {
    VERBS
        .iter()
        .filter(|v| v.words.len() <= args.len() && v.words.iter().zip(args).all(|(w, a)| w == a))
        .max_by_key(|v| v.words.len())
        .map(|v| (v, &args[v.words.len()..]))
}

fn table() -> String {
    let mut out = String::from(
        "humanify-ag-dev — the measurement harness (`cargo dev <verb>`; docs/dev-harness.md)\n\n",
    );
    for v in VERBS {
        let status = match v.lane {
            None => String::new(),
            Some(lane) => format!("  [not yet ported: lane {lane}; today: {}]", v.today),
        };
        out.push_str(&format!(
            "  {}{status}\n      usage:        {}\n      proves:       {}\n      cannot prove: {}\n",
            name(v),
            v.usage,
            v.proves,
            v.cannot_prove
        ));
    }
    out
}

/// What running `args` comes to: the exit code, stdout, stderr.
fn dispatch(args: &[String]) -> (u8, String, String) {
    if args.is_empty() || args == ["help"] || args == ["--help"] {
        return (0, table(), String::new());
    }
    let Some((verb, rest)) = find(args) else {
        return (
            2,
            String::new(),
            format!("unknown verb: {}\n\n{}", args.join(" "), table()),
        );
    };
    if rest.iter().any(|a| a == "--help") {
        return (
            0,
            format!("usage: humanify-ag-dev {}\n", verb.usage),
            String::new(),
        );
    }
    if let Some(lane) = verb.lane {
        return (
            2,
            String::new(),
            format!(
                "humanify-ag-dev {}: not yet ported (lane {lane}). Until it lands: {}\n",
                name(verb),
                verb.today
            ),
        );
    }
    match settings::parse(&name(verb), verb.settings, &[], rest) {
        Err(errors) => (
            2,
            String::new(),
            format!("{}: {}\n", name(verb), errors.join("\n")),
        ),
        Ok(parsed) => run(verb, &parsed),
    }
}

fn run(verb: &Verb, parsed: &settings::Parsed) -> (u8, String, String) {
    match verb.words {
        ["build-bin"] => build_bin(&parsed.positionals),
        _ => unreachable!("every implemented verb has a runner"),
    }
}

fn build_bin(positionals: &[String]) -> (u8, String, String) {
    let root = match positionals {
        [] => humanify_ag_dev::paths::repo_root(),
        [dir] => std::path::PathBuf::from(dir),
        _ => {
            return (
                2,
                String::new(),
                "usage: humanify-ag-dev build-bin [<checkout-dir>]\n".into(),
            );
        }
    };
    match bin_build::build(&root, &env::host().child_path()) {
        Ok(r) => (
            0,
            format!(
                "{{\"path\": {:?}, \"sha256\": {:?}, \"commit\": {:?}, \"dirty\": {}, \"name\": {:?}}}\n",
                r.path.display().to_string(),
                r.sha256,
                r.commit,
                r.dirty,
                r.name
            ),
            String::new(),
        ),
        Err(e) => (1, String::new(), format!("build-bin: {e}\n")),
    }
}

fn main() -> ExitCode {
    let (code, stdout, stderr) = dispatch(&env::user_args());
    print!("{stdout}");
    eprint!("{stderr}");
    ExitCode::from(code)
}

#[cfg(test)]
#[path = "main_test.rs"]
mod main_test;
