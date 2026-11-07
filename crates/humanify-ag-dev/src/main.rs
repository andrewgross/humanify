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
const DETERMINISM_SETTINGS: &[&str] = &["libscore.corpus", "libscore.data_root"];
const RESCORE_SETTINGS: &[&str] = &["libscore.corpus", "libscore.data_root", "libscore.out"];
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
        usage: "eval vendor-precision check <diag.json> --truth bun-dev:<id>|cc216|none  |  eval vendor-precision aggregate <label-dir>",
        proves: "app/wrong-package bytes among the truth-LABELLED vendor bytes, through the run's own raw-to-era bridge",
        cannot_prove: "anything about unlabelled bytes, or a run without the bridge (pre-exp166 binaries)",
        lane: None,
        today: "npm run eval -- vendor-precision",
        settings: &["libscore.truth"],
    },
    Verb {
        words: &["eval", "cjs-placement"],
        usage: "eval cjs-placement <census.jsonl> <runs-dir> [<out.json>]",
        proves: "where a CommonJS app's modules were placed against the bun-dev census",
        cannot_prove: "anything outside the census's bundles",
        lane: None,
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
        lane: Some("L3"),
        today: "npm run eval -- library",
        settings: LIBRARY_SETTINGS,
    },
    Verb {
        words: &["library", "rescore"],
        usage: "library rescore <label> [--corpus NAME] [--data-root DIR] [--out DIR]",
        proves: "the label's cards and summary from its raw discovery reports, byte for byte what score.py (scorer 14) writes",
        cannot_prove: "anything the reports do not hold: it re-scores, it never re-runs discovery",
        lane: None,
        today: "",
        settings: RESCORE_SETTINGS,
    },
    Verb {
        words: &["library", "proof"],
        usage: "library proof <label>... --out SCRATCH",
        proves: "re-scoring each committed label today reproduces its committed cards and summary byte for byte (proof b)",
        cannot_prove: "anything about a label whose raw reports or index are gone from this host",
        lane: None,
        today: "",
        settings: &["libscore.out"],
    },
    Verb {
        words: &["library", "leaderboard"],
        usage: "library leaderboard <label> <label> [label...] [--force-mixed]",
        proves: "relative library-matching movement between labels on the fixed corpus (band 0: every delta is real)",
        cannot_prove: "why a delta happened when libraryBytes moved too",
        lane: None,
        today: "npm run eval -- library-leaderboard",
        settings: FORCE,
    },
    Verb {
        words: &["library", "determinism"],
        usage: "library determinism <a> <b> [--corpus NAME] [--data-root DIR]",
        proves: "two labels of one configuration are byte-identical (the band-0 claim)",
        cannot_prove: "anything about two different configurations",
        lane: None,
        today: "python3 experiments/096-library-matching-score/determinism.py",
        settings: DETERMINISM_SETTINGS,
    },
    Verb {
        words: &["library", "corpus"],
        usage: "library corpus verify <corpus> [--data-root DIR]",
        proves: "a corpus's frozen files are byte-identical to its manifest",
        cannot_prove: "that the truth labels are correct",
        lane: None,
        today: "python3 experiments/096-library-matching-score/corpus.py",
        settings: &["libscore.data_root"],
    },
    Verb {
        words: &["library", "layers"],
        usage: "library layers <corpus> --out DIR [--data-root DIR]",
        proves: "re-freezing a corpus's truth layers reproduces every committed layer sha",
        cannot_prove: "that a layer's rule is the right rule",
        lane: None,
        today: "experiments/150-bun-testbed/scripts/layers.sh",
        settings: &["libscore.data_root", "libscore.out"],
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
        ["library", "rescore"] => library_rescore(parsed),
        ["library", "layers"] => library_layers(parsed),
        ["library", "proof"] => library_proof(parsed),
        ["library", "corpus"] => library_corpus(parsed),
        ["eval", "vendor-precision"] => vendor_precision(parsed),
        ["eval", "cjs-placement"] => cjs_placement(&parsed.positionals),
        ["library", "leaderboard"] => library_leaderboard(parsed),
        ["library", "determinism"] => library_determinism(parsed),
        _ => unreachable!("every implemented verb has a runner"),
    }
}

fn library_rescore(parsed: &settings::Parsed) -> (u8, String, String) {
    use humanify_ag_dev::library::score::{Out, rescore};
    let [label] = parsed.positionals.as_slice() else {
        return (2, String::new(), "usage: humanify-ag-dev library rescore <label> [--corpus NAME] [--data-root DIR] [--out DIR]\n".into());
    };
    let s = &parsed.settings;
    let corpus = s.text("libscore.corpus").unwrap_or_else(|| "dev".into());
    let out = s
        .text("libscore.out")
        .map_or(Out::Committed, |d| Out::Dir(d.into()));
    match rescore(
        &humanify_ag_dev::paths::repo_root(),
        label,
        &corpus,
        s.text("libscore.data_root").as_deref(),
        &out,
    ) {
        Ok(log) => (0, log, String::new()),
        Err(e) => (1, String::new(), format!("library rescore: {e}\n")),
    }
}

fn library_leaderboard(parsed: &settings::Parsed) -> (u8, String, String) {
    let repo = humanify_ag_dev::paths::repo_root();
    let bands = repo
        .join(humanify_ag_dev::paths::LIBRARY_DIR)
        .join("bands.json")
        .exists();
    let force = parsed.settings.flag_on("run.force_mixed");
    let results = repo.join(humanify_ag_dev::paths::LIBRARY_RESULTS);
    match humanify_ag_dev::library::leaderboard::leaderboard(
        &results,
        bands,
        &parsed.positionals,
        force,
    ) {
        Ok(text) => (0, text, String::new()),
        Err(e) => (2, String::new(), format!("{e}\n")),
    }
}

fn library_determinism(parsed: &settings::Parsed) -> (u8, String, String) {
    use humanify_ag_dev::library::{corpus::Corpus, determinism::determinism};
    let [a, b] = parsed.positionals.as_slice() else {
        return (
            2,
            String::new(),
            "usage: humanify-ag-dev library determinism <a> <b> [--corpus NAME]\n".into(),
        );
    };
    let repo = humanify_ag_dev::paths::repo_root();
    let s = &parsed.settings;
    let corpus_name = s.text("libscore.corpus").unwrap_or_else(|| "dev".into());
    let corpus = match Corpus::open(&repo, &corpus_name, s.text("libscore.data_root").as_deref()) {
        Ok(c) => c,
        Err(e) => return (2, String::new(), format!("{e}\n")),
    };
    let results = repo.join(humanify_ag_dev::paths::LIBRARY_RESULTS);
    match determinism(&corpus, &results, a, b) {
        Ok((line, bands)) => {
            let path = repo
                .join(humanify_ag_dev::paths::LIBRARY_DIR)
                .join("bands.json");
            match std::fs::write(&path, bands) {
                Ok(()) => (0, line, String::new()),
                Err(e) => (1, line, format!("{}: {e}\n", path.display())),
            }
        }
        Err((msg, code)) => (code, format!("{msg}\n"), String::new()),
    }
}

fn vendor_precision(parsed: &settings::Parsed) -> (u8, String, String) {
    use humanify_ag_dev::library::{families::Families, vendor_precision as vp};
    let usage = "usage: humanify-ag-dev eval vendor-precision check <diag.json> --truth bun-dev:<id>|cc216|none  |  aggregate <label-dir>\n";
    match parsed.positionals.as_slice() {
        [mode, dir] if mode == "aggregate" => match vp::aggregate(std::path::Path::new(dir)) {
            Ok(text) => (0, text, String::new()),
            Err(e) => (1, String::new(), format!("{e}\n")),
        },
        [mode, diag] if mode == "check" => {
            let Some(spec) = parsed.settings.text("libscore.truth") else {
                return (2, String::new(), usage.into());
            };
            let run = || -> Result<String, String> {
                let text = std::fs::read_to_string(diag).map_err(|e| format!("{diag}: {e}"))?;
                let d: serde_json::Value =
                    serde_json::from_str(&text).map_err(|e| format!("{diag}: {e}"))?;
                let repo = humanify_ag_dev::paths::repo_root();
                let truth = if let Some(id) = spec.strip_prefix("bun-dev:") {
                    Some(vp::truth_bundev(&repo, id)?)
                } else if spec == "cc216" {
                    Some(vp::truth_cc(std::path::Path::new(vp::CC_TRUTH))?)
                } else {
                    None
                };
                let fam = truth.as_ref().map(|_| Families::shipped());
                Ok(vp::card_text(&vp::check(&d, truth.as_ref(), fam.as_ref())?))
            };
            match run() {
                Ok(text) => (0, text, String::new()),
                Err(e) => (1, String::new(), format!("{e}\n")),
            }
        }
        _ => (2, String::new(), usage.into()),
    }
}

fn cjs_placement(positionals: &[String]) -> (u8, String, String) {
    use std::path::Path;
    let (census, runs, out) = match positionals {
        [c, r] => (c, r, None),
        [c, r, o] => (c, r, Some(o)),
        _ => {
            return (2, String::new(), "usage: humanify-ag-dev eval cjs-placement <census.jsonl> <runs-dir> [<out.json>]\n".into());
        }
    };
    match humanify_ag_dev::library::cjs_placement::score(Path::new(census), Path::new(runs)) {
        Ok((text, json)) => match out.map(|o| std::fs::write(o, json)) {
            Some(Err(e)) => (1, text, format!("{e}\n")),
            _ => (0, text, String::new()),
        },
        Err(e) => (1, String::new(), format!("{e}\n")),
    }
}

fn library_proof(parsed: &settings::Parsed) -> (u8, String, String) {
    let Some(out) = parsed.settings.text("libscore.out") else {
        return (
            2,
            String::new(),
            "usage: humanify-ag-dev library proof <label>... --out SCRATCH\n".into(),
        );
    };
    let repo = humanify_ag_dev::paths::repo_root();
    let (mut ok, mut text) = (true, String::new());
    for label in &parsed.positionals {
        match humanify_ag_dev::library::score::proof(&repo, label, std::path::Path::new(&out)) {
            Ok((same, line)) => {
                ok &= same;
                text.push_str(&line);
            }
            Err(e) => {
                ok = false;
                text.push_str(&format!("{label}: FAILED: {e}\n"));
            }
        }
    }
    (if ok { 0 } else { 1 }, text, String::new())
}

fn library_corpus(parsed: &settings::Parsed) -> (u8, String, String) {
    use humanify_ag_dev::library::corpus::{Corpus, verify};
    let [mode, name] = parsed.positionals.as_slice() else {
        return (
            2,
            String::new(),
            "usage: humanify-ag-dev library corpus verify <corpus> [--data-root DIR]\n".into(),
        );
    };
    if mode != "verify" {
        return (
            2,
            String::new(),
            format!(
                "library corpus {mode}: not ported (only `verify`; freezing a NEW corpus still runs corpus.py / kind_corpus.py)\n"
            ),
        );
    }
    let repo = humanify_ag_dev::paths::repo_root();
    match Corpus::open(
        &repo,
        name,
        parsed.settings.text("libscore.data_root").as_deref(),
    ) {
        Ok(c) => {
            let bad = verify(&c);
            if bad.is_empty() {
                (0, "corpus OK\n".into(), String::new())
            } else {
                (
                    1,
                    format!("{}\ncorpus BROKEN: {} file(s)\n", bad.join("\n"), bad.len()),
                    String::new(),
                )
            }
        }
        Err(e) => (2, String::new(), format!("{e}\n")),
    }
}

fn refrozen_line(r: &humanify_ag_dev::library::layers::Refrozen) -> String {
    format!(
        "{:12} {}/{} entries reproduce the committed sha; manifest {}{}\n",
        r.kind.prefix(),
        r.same,
        r.entries,
        if r.manifest_same {
            "identical"
        } else {
            "DIFFERS"
        },
        if r.differ.is_empty() {
            String::new()
        } else {
            format!("; differ: {}", r.differ.join(" "))
        }
    )
}

fn library_layers(parsed: &settings::Parsed) -> (u8, String, String) {
    use humanify_ag_dev::committed::library::LayerKind;
    use humanify_ag_dev::library::{corpus::Corpus, layers};
    let usage = "usage: humanify-ag-dev library layers <corpus> --out DIR [--data-root DIR]\n";
    let ([name], Some(out)) = (
        parsed.positionals.as_slice(),
        parsed.settings.text("libscore.out"),
    ) else {
        return (2, String::new(), usage.into());
    };
    let repo = humanify_ag_dev::paths::repo_root();
    let corpus = match Corpus::open(
        &repo,
        name,
        parsed.settings.text("libscore.data_root").as_deref(),
    ) {
        Ok(c) => c,
        Err(e) => return (2, String::new(), format!("{e}\n")),
    };
    let mut text = String::new();
    let mut ok = true;
    for layer in &layers::ORIGIN_LAYERS {
        let r = layers::refreeze_origin_layer(
            &corpus,
            layer,
            std::path::Path::new(&out),
            std::path::Path::new(layers::REGISTRY),
            std::path::Path::new(layers::BUNDLED),
        );
        match r {
            Ok(r) => {
                ok &= r.differ.is_empty() && r.manifest_same;
                text.push_str(&refrozen_line(&r));
            }
            Err(e) => {
                ok = false;
                text.push_str(&format!("{:12} FAILED: {e}\n", layer.kind.prefix()));
            }
        }
    }
    for kind in [
        LayerKind::Chained,
        LayerKind::Chained9,
        LayerKind::Chained11,
    ] {
        match layers::refreeze_chained_layer(
            &corpus,
            kind,
            std::path::Path::new(&out),
            std::path::Path::new(humanify_ag_dev::library::chained::NPM_REGISTRY),
        ) {
            Ok(r) => {
                ok &= r.differ.is_empty() && r.manifest_same;
                text.push_str(&refrozen_line(&r));
            }
            Err(e) => {
                ok = false;
                text.push_str(&format!("{:12} FAILED: {e}\n", kind.prefix()));
            }
        }
    }
    for (layer, builder) in layers::unported() {
        text.push_str(&format!(
            "{layer:12} not re-frozen (builder not ported: {builder})\n"
        ));
    }
    (if ok { 0 } else { 1 }, text, String::new())
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
