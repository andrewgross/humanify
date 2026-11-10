//! `humanify-ag-dev` (`cargo dev <verb>`): THE verb table of the
//! measurement harness (docs/dev-harness.md §2.4). Every instrument is one
//! entry with its usage, what it PROVES and what it CANNOT prove (a test
//! fails an entry without both, as measurement-owners.test.ts did for
//! eval.ts), the settings it reads, and — until its lane lands — the lane
//! that ports it. A verb not yet ported says so and exits non-zero; it
//! never half-runs. `cargo dev` with no arguments prints the table.

use std::process::ExitCode;

use humanify_ag_dev::{bin_build, census, e2e, env, eval, gate, match_truth, runners, settings};

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
    /// What it consumes, so the coordinating agent can decide what runs
    /// together (exp205: there is no host-wide lock). `LIGHT` for a verb
    /// that only reads and writes a few files.
    footprint: Footprint,
}

/// A verb's footprint: measured numbers where a run recorded them,
/// "unmeasured" otherwise.
#[derive(Debug, PartialEq, Eq)]
struct Footprint {
    /// Peak memory class.
    memory: &'static str,
    /// Whether it asks the LLM server (pairs.json's endpoint) or another model.
    llm: &'static str,
    /// Rough duration.
    time: &'static str,
    /// The folders it writes; a shared one is named.
    writes: &'static str,
}

const LIGHT: Footprint = Footprint {
    memory: "small",
    llm: "none",
    time: "seconds to minutes",
    writes: "only the output it names",
};

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
const RESCORE_SETTINGS: &[&str] = &["libscore.corpus", "libscore.data_root", "run.out"];
const WALK_SETTINGS: &[&str] = &[
    "run.bin",
    "run.force_mixed",
    "run.pipeline_set",
    "eval.library_index",
    "eval.inputs_base",
    "eval.boot_prompt",
    "llm.endpoint",
    "walk.dir",
    "walk.resume",
    "walk.reboot",
    "walk.report",
    "boot",
];
const INDEX_SECTION: &str = "index";
const INDEX_SETTINGS: &[&str] = &[INDEX_SECTION];
const NONE: &[&str] = &[];
const SCORER: &[&str] = &["scorer"];
const RESULTS: &[&str] = &["scorer.results"];
const FORCE_RESULTS: &[&str] = &["run.force_mixed", "scorer.results"];

const VERBS: &[Verb] = &[
    Verb {
        words: &["check"],
        usage: "check [--only a,b]",
        proves: "every gate stage passed on this tree (PARTIAL when --only)",
        cannot_prove: "anything about LLM-dependent output: the gate runs the pipeline against a stub",
        lane: None,
        today: "",
        settings: &["dev.only"],
        footprint: Footprint {
            memory: "cargo test + release build, then the e2e fixtures (small); unmeasured",
            llm: "none (the stub)",
            time: "~10-13 min on a loaded host (exp205: unit 330-341 s, release build 166-180 s, e2e 92-179 s)",
            writes: "this checkout's target/; e2e into a fresh temp dir",
        },
    },
    Verb {
        words: &["eval", "score"],
        usage: "eval score <label> [--pairs a->b,...] [--base-mode scratch|seeded|archive] [--llm-cache D] [--force-mixed] [--bin PATH] [--workdir D] [--library-index D] [--no-boot-prompt] [--no-self-hop] [--parallel-pairs N] [--parallel-rebases R] [--pipeline-set section.key=value]...",
        proves: "how the CURRENT TREE's cross-version diff decomposes (KPIs), pipeline exit, boot",
        cannot_prove: "any delta inside the measured noise-bands.json floor — it will still print a sign",
        lane: None,
        today: "",
        settings: SCORE_SETTINGS,
        footprint: Footprint {
            memory: "up to 5 pipeline runs at once (--parallel-pairs 5): summed RSS peak 26.6-30.8 GB, one run 6-19 GB (rebases 6.0-10.7, scored legs 9.7-17.7, self-hops 18.5-18.8; ref-names-40921b9f r1-r3, exp207)",
            llm: "yes: every rebase, scored leg and cold self-hop asks the LLM server, cold (the warm self-hop replays); ONE rebase at a time by default (--parallel-rebases 1: one rebase at 472 saturates the 4-replica server; five at once made it slower in total), scored legs overlap it at 10-35 in flight; the boot gate asks the Claude API (boot.model)",
            time: "~95 min for the five pairs in parallel lanes, ~119 min serial (--parallel-pairs 1), both stall-free the same night (ref-names-40921b9f-r3, serial-40921b9f-s1; exp207); a scored leg 188-546 s",
            writes: "<workdir>/<label>/ (default /work/eval-work/<label>; claimed while it runs), dev/eval/results/<label>/, this checkout's target/release",
        },
    },
    Verb {
        words: &["eval", "rescore"],
        usage: "eval rescore <label> --cards <dir> --work <dir> [--results <dir>]",
        proves: "what the current scorer (v2) reads off a label's recorded trees, every partition checked, old -> new per KPI",
        cannot_prove: "anything about the pipeline: no binary runs",
        lane: None,
        today: "",
        settings: SCORER,
        footprint: LIGHT,
    },
    Verb {
        words: &["eval", "summarize"],
        usage: "eval summarize <label> [--results <dir>]",
        proves: "nothing new — a re-render of recorded cards and verdicts; plus each pair's LLM call DIAGNOSTICS from its stats `llmCalls` block (server: sent/served, retries by reason, attempt latency p50/p90/p99/max; model answers: raw verdicts per asked identifier — proposed / taken / duplicate / invalid / unchanged / missing / equals the prior's hint; backups and lost answers)",
        cannot_prove: "validity of cards recorded without run-status (UNKNOWN, not clean); that a diagnostic delta MATTERS — diagnostics have no band and grade nothing, and answer verdicts are RAW (before the pipeline's repairs); runs before exp206 print '-'",
        lane: None,
        today: "",
        settings: RESULTS,
        footprint: LIGHT,
    },
    Verb {
        words: &["eval", "leaderboard"],
        usage: "eval leaderboard <label> [label...] [--force-mixed] [--results <dir>]",
        proves: "relative KPI movement between labels whose identities the guard accepts (base mode, naming mode, index, scorer version, model, pair inputs), over the pairs every label scored (a different pair set prints `!! COMMON PAIRS` and re-folds the totals over the common pairs)",
        cannot_prove: "that a sub-noise-floor delta is real; only novel/realLn have proven draw-invariance. Its LLM call diagnostics (server speed, model answers, backups, lost answers) explain a KPI move but are not KPIs: no band, latency is the worst pair's percentile, and answer counts move with the model's draws between cold runs ('-' for labels before exp206)",
        lane: None,
        today: "",
        settings: FORCE_RESULTS,
        footprint: LIGHT,
    },
    Verb {
        words: &["eval", "bands"],
        usage: "eval bands <label> <label> [label...] [--results <dir>]",
        proves: "how much two runs of IDENTICAL code disagree per KPI — the floor a delta must clear (written per scorer version and naming mode)",
        cannot_prove: "anything from labels at different commits, modes, scorer versions or pair sets — that measures a change, not a floor (refused)",
        lane: None,
        today: "",
        settings: RESULTS,
        footprint: LIGHT,
    },
    Verb {
        words: &["eval", "diff"],
        usage: "eval diff <priorTree> <freshTree> [label]",
        proves: "what the on-disk diff between THESE two trees is made of, git-capped",
        cannot_prove: "which mechanism caused a line",
        lane: None,
        today: "",
        settings: NONE,
        footprint: LIGHT,
    },
    Verb {
        words: &["eval", "neutrality"],
        usage: "eval neutrality <baseline-ref> [from:to] [--workdir D] [--cache D] [--priors D] [--inputs-base D] [--endpoint URL] [--library-index D] [--pipeline-set section.key=value]...",
        proves: "a refactor changed NOTHING: 0 differing files/lines, baseline leg wrote 0 cache entries",
        cannot_prove: "anything from a COLD run (baseline leg wrote entries) — cold verdicts are void, null-control proven 2026-08-11",
        lane: None,
        today: "",
        settings: NEUTRALITY_SETTINGS,
        footprint: Footprint {
            memory: "two pipeline runs one after the other (the same class: ~10-17 GB on a Claude Code bundle) and two release builds",
            llm: "replays the warm cache; a miss asks the LLM server (a valid verdict has the baseline leg asking nothing)",
            time: "~25 min per pair, both legs (warm)",
            writes: "<workdir>/neutrality-* and the baseline worktree there (default /work), and the cache (default <workdir>/neutrality-cache): both claimed while it runs, so a second run on the same cache or workdir is refused",
        },
    },
    Verb {
        words: &["eval", "vendor-precision"],
        usage: "eval vendor-precision check <diag.json> --truth bun-dev:<id>|cc216|none  |  aggregate <label-dir>  |  bun-dev <bin> <label> [--jobs N] [--out DIR] [--library-index D]",
        proves: "app/wrong-package bytes among the truth-LABELLED vendor bytes, through the run's own raw-to-era bridge (bun-dev: the whole corpus, names-mode stub runs)",
        cannot_prove: "anything about unlabelled bytes, or a run without the bridge (pre-exp166 binaries)",
        lane: None,
        today: "",
        settings: &[
            "libscore.truth",
            "libscore.jobs",
            "run.out",
            "eval.library_index",
        ],
        footprint: Footprint {
            memory: "bun-dev: --jobs N (default 4) names-mode stub runs at once; per-run peak unmeasured",
            llm: "none (the stub)",
            time: "bun-dev: unmeasured",
            writes: "bun-dev: --out (default /work/exp166-data/bundev/<label>; claimed while it runs)",
        },
    },
    Verb {
        words: &["eval", "cjs-placement"],
        usage: "eval cjs-placement <census.jsonl> <runs-dir> [<out.json>]  |  runs <bin> <label> <names|default> [--jobs N] [--out DIR]  |  trees <before-runs> <after-runs> <substring>...",
        proves: "where a CommonJS app's modules were placed against the bun-dev census (runs: the stub runs it reads; trees: their output trees side by side)",
        cannot_prove: "anything outside the census's bundles",
        lane: None,
        today: "",
        settings: &["libscore.jobs", "run.out", "eval.library_index"],
        footprint: Footprint {
            memory: "runs: --jobs N (default 4) stub runs at once; per-run peak unmeasured",
            llm: "none (the stub)",
            time: "runs: unmeasured",
            writes: "runs: --out (default /work/exp172-data/bundev/<label>; claimed while it runs)",
        },
    },
    Verb {
        words: &["eval", "walk"],
        usage: "eval walk <from>..<to>|<v>,<v>,... --walk-dir D [--resume] [--reboot] [--bin PATH] [--library-index D] [--no-boot-prompt] [--no-report] [--force-mixed] [--pipeline-set section.key=value]...",
        proves: "how the diff behaves hop after hop on the pipeline's OWN previous output, every hop booted and scored",
        cannot_prove: "an A/B between pipelines: each walk's hop N inherits its own hop N-1, and the eval's noise bands are for TOTALS over the eval's pairs, not walk hops",
        lane: None,
        today: "",
        settings: WALK_SETTINGS,
        footprint: Footprint {
            memory: "one pipeline run at a time: 17.3 GB peak RSS (walk 9ce9dee2, 124 hops)",
            llm: "yes: every hop asks the LLM server, cold; each hop's boot gate asks the Claude API (boot.model)",
            time: "89-490 s per hop (median 188 s, walk 9ce9dee2), plus each hop's boot gate and scoring",
            writes: "--walk-dir (claimed while it runs; a --resume of a live walk is refused)",
        },
    },
    Verb {
        words: &["eval", "walk-report"],
        usage: "eval walk-report <name>=<walk-dir> [<name>=<walk-dir>...] [--report-dir D]",
        proves: "the per-hop tables of finished walks side by side, and the hops whose noise or vendor hash turnover is far above the median",
        cannot_prove: "why a flagged hop is noisy: the flag says where to read, not what happened",
        lane: None,
        today: "",
        settings: &["walk.report_dir"],
        footprint: LIGHT,
    },
    Verb {
        words: &["run", "stub"],
        usage: "run stub <bin> <bundle> <out> [<prior.js>] [--library-index D] [--pipeline-set library.matching=off]",
        proves: "what the binary does on a bundle with every name answered by the one stub (no LLM)",
        cannot_prove: "anything about LLM naming quality",
        lane: None,
        today: "",
        settings: &["run.pipeline_set", "eval.library_index"],
        footprint: Footprint {
            memory: "one pipeline run (on a Claude Code bundle the scored-leg class, ~10-17 GB; a fixture far less)",
            llm: "none (the stub)",
            time: "unmeasured (one pipeline run)",
            writes: "<out>",
        },
    },
    Verb {
        words: &["library", "score"],
        usage: "library score <label> --index <dir> --rule agreement|structural [--commit REF] [--jobs N] [--cpus LIST] [--corpus NAME] [--sets a,b] [--force-mixed] [--pipeline-set library.<key>=value]...",
        proves: "precision / recall / F0.5 of library naming on a frozen corpus, and that no app code was claimed",
        cannot_prove: "anything about a bundle outside the corpus",
        lane: None,
        today: "",
        settings: LIBRARY_SETTINGS,
        footprint: Footprint {
            memory: "--jobs N (default 4) discover runs at once, each loading the index lookup (~2 GB dev, ~4 GB test: docs/design/library-index.md); unmeasured on the full index",
            llm: "none",
            time: "unmeasured",
            writes: "<corpus data>/results/<label>/ and dev/library/results/<label>/",
        },
    },
    Verb {
        words: &["library", "rescore"],
        usage: "library rescore <label> [--corpus NAME] [--data-root DIR] [--out DIR]",
        proves: "the label's cards and summary from its raw discovery reports, byte for byte what score.py (scorer 14) writes",
        cannot_prove: "anything the reports do not hold: it re-scores, it never re-runs discovery",
        lane: None,
        today: "",
        settings: RESCORE_SETTINGS,
        footprint: LIGHT,
    },
    Verb {
        words: &["library", "proof"],
        usage: "library proof <label>... --out SCRATCH",
        proves: "re-scoring each committed label today reproduces its committed cards and summary byte for byte (proof b)",
        cannot_prove: "anything about a label whose raw reports or index are gone from this host",
        lane: None,
        today: "",
        settings: &["run.out"],
        footprint: LIGHT,
    },
    Verb {
        words: &["library", "leaderboard"],
        usage: "library leaderboard <label> <label> [label...] [--force-mixed]",
        proves: "relative library-matching movement between labels on the fixed corpus (band 0: every delta is real)",
        cannot_prove: "why a delta happened when libraryBytes moved too",
        lane: None,
        today: "",
        settings: FORCE,
        footprint: LIGHT,
    },
    Verb {
        words: &["library", "npm"],
        usage: "library npm <corpus> [--npm-registry DIR] [--npm-bundled DIR] [--packages a,b] [--jobs N]",
        proves: "the npm caches the truth layers read hold every manifest and license collection the corpus needs (resumable: a cached file is never fetched again)",
        cannot_prove: "that the registry still answers as it did when a layer was frozen",
        lane: None,
        today: "",
        settings: &[
            "libscore.bundled",
            "libscore.data_root",
            "libscore.jobs",
            "libscore.packages",
            "libscore.registry",
            "run.out",
        ],
        footprint: Footprint {
            memory: "small",
            llm: "none; fetches from the npm registry over the network (it rate-limits bulk sweeps, as does an index build's dry run)",
            time: "unmeasured (resumable)",
            writes: "the npm caches (default /work/exp112-data/registry and /work/exp112-data/bundled; shared, never re-fetched)",
        },
    },
    Verb {
        words: &["library", "tokens14"],
        usage: "library tokens14 < request.json",
        proves: "scorer 14's name evidence (tokens14.mjs's protocol: units / files mode) from the ported acorn tokenizer",
        cannot_prove: "that the evidence is the right evidence for the rule",
        lane: None,
        today: "",
        settings: &[],
        footprint: LIGHT,
    },
    Verb {
        words: &["library", "determinism"],
        usage: "library determinism <a> <b> [--corpus NAME] [--data-root DIR]",
        proves: "two labels of one configuration are byte-identical (the band-0 claim)",
        cannot_prove: "anything about two different configurations",
        lane: None,
        today: "",
        settings: DETERMINISM_SETTINGS,
        footprint: LIGHT,
    },
    Verb {
        words: &["library", "corpus"],
        usage: "library corpus verify|freeze|build <corpus> [--out DIR] [--data-root DIR] [--jobs N]",
        proves: "verify: a corpus's frozen files are byte-identical to its manifest; freeze: re-freezing its inputs reproduces the manifest and origin layer; build: its bundles built from the apps' installs",
        cannot_prove: "that the truth labels are correct",
        lane: None,
        today: "",
        settings: &[
            "libscore.apps",
            "libscore.data_root",
            "libscore.jobs",
            "run.out",
        ],
        footprint: Footprint {
            memory: "build: --jobs N (default 4) bundler builds at once; unmeasured. verify / freeze: small",
            llm: "none",
            time: "build: unmeasured. verify / freeze: minutes",
            writes: "build: the corpus's data root (--out; default /work/exp150-data/bun or /work/exp181-data). freeze: --out",
        },
    },
    Verb {
        words: &["library", "layers"],
        usage: "library layers <corpus> --out DIR [--layers a,b] [--units L,...] [--index DIR] [--data-root DIR]",
        proves: "re-freezing a corpus's truth layers reproduces every committed layer sha",
        cannot_prove: "that a layer's rule is the right rule",
        lane: None,
        today: "",
        settings: &[
            "libscore.data_root",
            "libscore.index",
            "libscore.jobs",
            "libscore.layers",
            "libscore.units",
            "run.out",
        ],
        footprint: LIGHT,
    },
    Verb {
        words: &["index", "build"],
        usage: "index build <spec> --registry D --store D --index-work D --index-out D [--chunks N] [--index-jobs N] [--merge-jobs N]",
        proves: "a library index built from a committed spec, chunked, with one merge",
        cannot_prove: "that the index's packages are the right ones",
        lane: None,
        today: "",
        settings: INDEX_SETTINGS,
        footprint: Footprint {
            memory: "--chunks (default 8) chunks at once, each --index-jobs package builds: one huge package spiked a chunk to 55-65 GB (pinyin, exp152); the merge peaked 117 GB serial (exp152, full Bun index) and 103.6 GB at --merge-jobs 30 (exp177)",
            llm: "none; the dry run and the installs reach the npm registry",
            time: "6.9 h for the full Bun index (exp152: 5.6 min registry, 6.08 h shards, 50 min merge)",
            writes: "--store and --index-work (both claimed while it runs), --registry (shared on purpose), --index-out",
        },
    },
    Verb {
        words: &["boot-gate"],
        usage: "boot-gate <tree> <version>",
        proves: "a humanified Claude Code tree boots (--version) and answers a live prompt",
        cannot_prove: "behaviour beyond the boot and one prompt",
        lane: None,
        today: "",
        settings: &["boot"],
        footprint: Footprint {
            memory: "one Claude Code process; unmeasured",
            llm: "one live prompt to the Claude API (boot.model), not the LLM server",
            time: "unmeasured (seconds to a minute)",
            writes: "nothing but its report line",
        },
    },
    Verb {
        words: &["build-bin"],
        usage: "build-bin [<checkout-dir> | <ref>] [--workdir D]",
        proves: "which binary a checkout or a commit builds: its path, sha256, build commit and dirt (the package and binary name read from THAT commit; a ref is built in a detached worktree under <workdir>/build-bin, refused before the cutover)",
        cannot_prove: "anything about what the binary does",
        lane: None,
        today: "",
        settings: &["run.workdir"],
        footprint: Footprint {
            memory: "one cargo release build; unmeasured",
            llm: "none",
            time: "unmeasured (a release build)",
            writes: "<workdir>/build-bin/<sha>/ (default /work/build-bin; one worktree per commit) and its target/",
        },
    },
    Verb {
        words: &["e2e"],
        usage: "e2e [--out D]",
        proves: "the release binary on every committed fixture: fresh, prior, deterministic twice, and the exact export surface",
        cannot_prove: "LLM naming quality: the names come from a stub",
        lane: None,
        today: "",
        settings: &["run.out"],
        footprint: Footprint {
            memory: "the release binary on the committed fixtures (small); unmeasured",
            llm: "none (the stub)",
            time: "unmeasured (minutes)",
            writes: "--out, else a fresh temp dir",
        },
    },
    Verb {
        words: &["format-golden"],
        usage: "format-golden",
        proves: "the formatter's bytes equal the frozen goldens, and a planted perturbation is caught",
        cannot_prove: "formatting of inputs outside the goldens",
        lane: None,
        today: "",
        settings: NONE,
        footprint: LIGHT,
    },
    Verb {
        words: &["census", "clones"],
        usage: "census clones [--loose] [--root D]",
        proves: "where twin functions sit across crates (ADVISORY)",
        cannot_prove: "that two twins answer the same question the same way",
        lane: None,
        today: "",
        settings: &["dev.loose", "dev.root"],
        footprint: LIGHT,
    },
    Verb {
        words: &["census", "concurrency"],
        usage: "census concurrency [--files a.rs,...] [--root D] [--allowlist F]",
        proves: "every concurrency site in non-test crates/ is confirmed on the allowlist with its pattern",
        cannot_prove: "that a confirmed site actually follows its pattern's rules",
        lane: None,
        today: "",
        settings: &["dev.files", "dev.root", "dev.allowlist"],
        footprint: LIGHT,
    },
    Verb {
        words: &["census", "slop"],
        usage: "census slop [--base REF] [--root D]",
        proves: "per-crate complexity mass, erosion and clone share against main (ADVISORY)",
        cannot_prove: "correctness: a measure of style",
        lane: None,
        today: "",
        settings: &["dev.base", "dev.root"],
        footprint: LIGHT,
    },
    Verb {
        words: &["ask-trace"],
        usage: "ask-trace <input.js> [--out D] [--prior P] [--collide NAME] [--sequential] [--pipeline-set section.key=value]... [-- <pipeline flags>]",
        proves: "every ask the binary makes on a fixture, against the stub",
        cannot_prove: "what a real model would answer",
        lane: None,
        today: "",
        settings: &[
            "run.out",
            "dev.prior",
            "dev.collide",
            "dev.sequential",
            "run.pipeline_set",
        ],
        footprint: Footprint {
            memory: "one pipeline run on a fixture (small)",
            llm: "none (the stub)",
            time: "seconds",
            writes: "--out",
        },
    },
    Verb {
        words: &["diff-asks"],
        usage: "diff-asks <a> <b>",
        proves: "how two ask dumps differ",
        cannot_prove: "why they differ",
        lane: None,
        today: "",
        settings: NONE,
        footprint: LIGHT,
    },
    Verb {
        words: &["match-truth"],
        usage: "match-truth [--corpus-dir D] [--bin PATH] [--out D] [--only a,b]",
        proves: "the matcher's agreement with ground truth on a pinned npm corpus",
        cannot_prove: "matching on bundles outside the corpus",
        lane: None,
        today: "",
        settings: &["dev.corpus_dir", "run.bin", "run.out", "dev.only"],
        footprint: Footprint {
            memory: "one match run per corpus package; unmeasured",
            llm: "none; fetches the pinned npm corpus once",
            time: "unmeasured",
            writes: "the corpus dir (default /work/match-truth-corpus, shared download cache) and --out, else a fresh /work/match-truth-runs/run-NNNN",
        },
    },
    Verb {
        words: &["env-check"],
        usage: "env-check [--endpoint URL]",
        proves: "the toolchain pin, clippy and rustfmt are present and the MSRV equals the pin",
        cannot_prove: "anything about the code",
        lane: None,
        today: "",
        settings: &["llm.endpoint"],
        footprint: LIGHT,
    },
    Verb {
        words: &["precommit"],
        usage: "precommit",
        proves: "staged files are formatted and pass the staged-file censuses",
        cannot_prove: "that the gate passes: the hook is not the gate",
        lane: None,
        today: "",
        settings: NONE,
        footprint: LIGHT,
    },
    Verb {
        words: &["stub-llm", "serve"],
        usage: "stub-llm serve [--port N]",
        proves: "nothing: it serves the one deterministic stub LLM for by-hand runs",
        cannot_prove: "anything about a real model",
        lane: None,
        today: "",
        settings: &["dev.port"],
        footprint: LIGHT,
    },
    Verb {
        words: &["no-private-addresses"],
        usage: "no-private-addresses",
        proves: "no tracked file holds a private (RFC 1918 / link-local) network address — the repo is public",
        cannot_prove: "anything about untracked files, or a private host NAME",
        lane: None,
        today: "",
        settings: NONE,
        footprint: LIGHT,
    },
    Verb {
        words: &["shims"],
        usage: "shims",
        proves: "the remaining JS shims (the webcrack shim the pipeline runs) parse",
        cannot_prove: "that a shim behaves: the e2e stage runs it",
        lane: None,
        today: "",
        settings: NONE,
        footprint: LIGHT,
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
        let f = &v.footprint;
        if *f == LIGHT {
            out.push_str(
                "      footprint:    light (small, no LLM, writes only its named output)\n",
            );
        } else {
            out.push_str(&format!(
                "      footprint:    memory: {}\n                    llm:    {}\n                    time:   {}\n                    writes: {}\n",
                f.memory, f.llm, f.time, f.writes
            ));
        }
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

/// The runners' live context: this binary's checkout and the host.
fn live() -> runners::Ctx {
    let cwd = env::current_dir().unwrap_or_default();
    let repo = humanify_ag_dev::paths::repo_root();
    let repo = repo.canonicalize().unwrap_or(repo);
    runners::Ctx::new(repo, env::host(), cwd)
}

fn run(verb: &Verb, parsed: &settings::Parsed) -> (u8, String, String) {
    use runners::verbs;
    let done = |code: u8| (code, String::new(), String::new());
    let host = env::host();
    match verb.words {
        ["build-bin"] => build_bin(parsed),
        ["check"] => gate::check::run(parsed, &host),
        ["env-check"] => gate::env_check::run(parsed, &host),
        ["precommit"] => gate::precommit::run(parsed, &host),
        ["format-golden"] => gate::format_golden::run(parsed, &host),
        ["no-private-addresses"] => gate::private_addresses::run(parsed, &host),
        ["shims"] => gate::shims::run(parsed, &host),
        ["census", "concurrency"] => census::concurrency::run(parsed, &host),
        ["census", "clones"] => census::clones::run(parsed, &host),
        ["census", "slop"] => census::slop::run(parsed, &host),
        ["e2e"] => e2e::run::run(parsed, &host),
        ["ask-trace"] => e2e::ask_trace::run(parsed, &host),
        ["diff-asks"] => e2e::diff_asks::run(parsed, &host),
        ["stub-llm", "serve"] => e2e::stub_llm::run(parsed, &host),
        ["match-truth"] => match_truth::run(parsed, &host),
        ["eval", "rescore"] => eval::verbs::rescore(parsed),
        ["eval", "summarize"] => eval::verbs::summarize(parsed),
        ["eval", "leaderboard"] => eval::verbs::leaderboard(parsed),
        ["eval", "bands"] => eval::verbs::bands(parsed),
        ["eval", "diff"] => eval::verbs::diff(parsed),
        ["library", "rescore"] => library_rescore(parsed),
        ["library", "layers"] => library_layers(parsed),
        ["library", "proof"] => library_proof(parsed),
        ["library", "corpus"] => library_corpus(parsed),
        ["eval", "vendor-precision"] => vendor_precision(parsed),
        ["eval", "cjs-placement"] => cjs_placement(parsed),
        ["library", "leaderboard"] => library_leaderboard(parsed),
        ["library", "determinism"] => library_determinism(parsed),
        ["library", "tokens14"] => library_tokens14(),
        ["library", "npm"] => library_npm(parsed),
        ["eval", "score"] => done(verbs::score(&live(), parsed, verb.settings)),
        ["eval", "neutrality"] => done(verbs::neutrality(&live(), parsed, verb.settings)),
        ["eval", "walk"] => done(verbs::walk(&live(), parsed)),
        ["eval", "walk-report"] => done(verbs::walk_report(&live(), parsed)),
        ["run", "stub"] => done(verbs::stub(&live(), parsed)),
        ["index", "build"] => done(verbs::index_build(&live(), parsed)),
        ["boot-gate"] => done(verbs::boot_gate(&live(), parsed)),
        ["library", "score"] => done(verbs::library_score(&live(), parsed)),
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
        .text("run.out")
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

fn library_npm(parsed: &settings::Parsed) -> (u8, String, String) {
    use humanify_ag_dev::library::{corpus::Corpus, npm};
    let [name] = parsed.positionals.as_slice() else {
        return (
            2,
            String::new(),
            "usage: humanify-ag-dev library npm <corpus>\n".into(),
        );
    };
    let s = &parsed.settings;
    let corpus = match Corpus::open(
        &humanify_ag_dev::paths::repo_root(),
        name,
        s.text("libscore.data_root").as_deref(),
    ) {
        Ok(c) => c,
        Err(e) => return (2, String::new(), format!("{e}\n")),
    };
    let registry = s
        .text("libscore.registry")
        .unwrap_or_else(|| npm::REGISTRY.into());
    let bundled = s
        .text("libscore.bundled")
        .unwrap_or_else(|| npm::BUNDLED.into());
    let jobs = s.int("libscore.jobs").unwrap_or(4) as usize;
    let only = s.list("libscore.packages");
    let record = s.text("run.out").map(std::path::PathBuf::from);
    match npm::fill(
        &corpus,
        std::path::Path::new(&registry),
        std::path::Path::new(&bundled),
        jobs,
        only.as_deref(),
        record.as_deref(),
    ) {
        Ok((text, fails)) => (u8::from(fails > 0), text, String::new()),
        Err(e) => (1, String::new(), format!("{e}\n")),
    }
}

fn library_tokens14() -> (u8, String, String) {
    use std::io::Read;
    let mut req = String::new();
    if let Err(e) = std::io::stdin().read_to_string(&mut req) {
        return (2, String::new(), format!("stdin: {e}\n"));
    }
    match humanify_ag_dev::library::tokens14::serve(&req) {
        Ok(out) => (0, out, String::new()),
        Err(e) => (2, String::new(), format!("tokens14: {e}\n")),
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
        [mode, bin, label] if mode == "bun-dev" => {
            let ctx = live();
            let index = match batch_index(&ctx, parsed) {
                Ok(i) => i,
                Err(e) => return (2, String::new(), format!("{e}\n")),
            };
            let out = batch_out(parsed, "/work/exp166-data/bundev", label);
            let jobs = parsed.settings.int("libscore.jobs").unwrap_or(4) as usize;
            match runners::batch::vendor_precision_bundev(
                &ctx,
                std::path::Path::new(bin),
                &out,
                jobs,
                &index,
            ) {
                Ok(text) => (
                    if text.contains("FAIL") { 1 } else { 0 },
                    text,
                    String::new(),
                ),
                Err(e) => (2, String::new(), format!("{e}\n")),
            }
        }
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

/// The index a batch's names-mode stub runs read: `--library-index`, else
/// pairs.json's `libraryIndex` (the eval's prebuilt one).
fn batch_index(ctx: &runners::Ctx, parsed: &settings::Parsed) -> Result<String, String> {
    if let Some(i) = parsed.settings.text("eval.library_index") {
        return Ok(ctx.absolute(&i).to_string_lossy().to_string());
    }
    humanify_ag_dev::committed::eval::pairs(&ctx.repo).map(|p| p.value.library_index)
}

fn batch_out(parsed: &settings::Parsed, default_root: &str, label: &str) -> std::path::PathBuf {
    parsed.settings.text("run.out").map_or_else(
        || std::path::Path::new(default_root).join(label),
        std::path::PathBuf::from,
    )
}

fn cjs_placement(parsed: &settings::Parsed) -> (u8, String, String) {
    use std::path::Path;
    let positionals = parsed.positionals.as_slice();
    let jobs = parsed.settings.int("libscore.jobs").unwrap_or(4) as usize;
    match positionals {
        [mode, bin, label, naming] if mode == "runs" => {
            let ctx = live();
            let index = match naming.as_str() {
                "names" => match batch_index(&ctx, parsed) {
                    Ok(i) => Some(i),
                    Err(e) => return (2, String::new(), format!("{e}\n")),
                },
                "default" => None,
                other => {
                    return (
                        2,
                        String::new(),
                        format!("cjs-placement runs: mode {other}: names or default\n"),
                    );
                }
            };
            let out = batch_out(parsed, "/work/exp172-data/bundev", label);
            return match runners::batch::cjs_runs(
                &ctx,
                Path::new(bin),
                &out,
                jobs,
                index.as_deref(),
            ) {
                Ok(text) => (
                    if text.contains("FAIL") { 1 } else { 0 },
                    text,
                    String::new(),
                ),
                Err(e) => (2, String::new(), format!("{e}\n")),
            };
        }
        [mode, before, after, wanted @ ..] if mode == "trees" && !wanted.is_empty() => {
            return match runners::batch::trees(Path::new(before), Path::new(after), wanted) {
                Ok(text) => (0, text, String::new()),
                Err(e) => (1, String::new(), format!("{e}\n")),
            };
        }
        _ => {}
    }
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
    let Some(out) = parsed.settings.text("run.out") else {
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
    let repo = humanify_ag_dev::paths::repo_root();
    if mode == "freeze" {
        let Some(out) = parsed.settings.text("run.out") else {
            return (
                2,
                String::new(),
                "usage: humanify-ag-dev library corpus freeze <corpus> --out DIR\n".into(),
            );
        };
        return match humanify_ag_dev::library::freeze::run(&repo, name, std::path::Path::new(&out))
        {
            Ok((ok, text)) => (if ok { 0 } else { 1 }, text, String::new()),
            Err(e) => (2, String::new(), format!("{e}\n")),
        };
    }
    if mode == "build" {
        return library_corpus_build(parsed, name);
    }
    if mode != "verify" {
        return (
            2,
            String::new(),
            format!("library corpus {mode}: the modes are build, freeze and verify\n"),
        );
    }
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
    if !r.committed {
        return format!(
            "{:12} {} entries built; no committed manifest (a NEW layer: review, then commit its manifest)\n",
            r.kind.prefix(),
            r.entries
        );
    }
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

fn library_corpus_build(parsed: &settings::Parsed, name: &str) -> (u8, String, String) {
    use humanify_ag_dev::library::{corpus_build, freeze};
    let root = parsed.settings.text("run.out").unwrap_or_else(|| {
        if name.starts_with("kind-") {
            freeze::KIND_DATA.to_string()
        } else {
            freeze::BUN_APPS.to_string()
        }
    });
    let jobs = parsed.settings.int("libscore.jobs").unwrap_or(4) as usize;
    let apps = parsed.settings.list("libscore.apps");
    match corpus_build::build(name, std::path::Path::new(&root), jobs, apps.as_deref()) {
        Ok(text) => (
            if text.contains("FAILED") { 1 } else { 0 },
            text,
            String::new(),
        ),
        Err(e) => (2, String::new(), format!("{e}\n")),
    }
}

fn library_layers(parsed: &settings::Parsed) -> (u8, String, String) {
    use humanify_ag_dev::library::{corpus::Corpus, layers};
    let usage = "usage: humanify-ag-dev library layers <corpus> --out DIR [--layers a,b] [--units L,...] [--index DIR] [--data-root DIR]\n";
    let ([name], Some(out)) = (
        parsed.positionals.as_slice(),
        parsed.settings.text("run.out"),
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
    let want = parsed.settings.list("libscore.layers");
    let opts = layers::Options {
        units: parsed.settings.list("libscore.units"),
        index: parsed
            .settings
            .text("libscore.index")
            .map(std::path::PathBuf::from),
        jobs: parsed.settings.int("libscore.jobs").unwrap_or(4) as usize,
    };
    let plan = match layers::plan(want.as_deref()) {
        Ok(p) => p,
        Err(e) => return (2, String::new(), format!("{e}\n")),
    };
    let mut text = String::new();
    let mut ok = true;
    for kind in plan {
        match layers::refreeze(&corpus, kind, std::path::Path::new(&out), &opts) {
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
    (if ok { 0 } else { 1 }, text, String::new())
}

fn build_bin(parsed: &settings::Parsed) -> (u8, String, String) {
    let positionals = &parsed.positionals;
    if let [arg] = positionals.as_slice()
        && !std::path::Path::new(arg).is_dir()
    {
        return runners::verbs::build_bin_ref(&live(), parsed, arg);
    }
    let root = match positionals.as_slice() {
        [] => humanify_ag_dev::paths::repo_root(),
        [dir] => std::path::PathBuf::from(dir),
        _ => {
            return (
                2,
                String::new(),
                "usage: humanify-ag-dev build-bin [<checkout-dir> | <ref>]\n".into(),
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
