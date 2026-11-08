//! `cargo dev check [--only a,b]`: THE gate — ONE command that runs every
//! check and reports what it ran (the successor of `scripts/check.ts`,
//! docs/dev-harness.md §4 d).
//!
//! There used to be three commands and none of them ran everything: e2e
//! sat outside the documented gate and knip outside the one people ran,
//! so a gate was red on main for two findings nobody had seen
//! (measurement-pitfalls rule 8 in the test suite). The fix was one
//! command with nothing outside it, and this is that command.
//!
//! **A stage that did not run must never look like a stage that passed.**
//! Every stage reports passed / failed / review / skipped, the summary
//! prints all of them, a fail-fast break lists every stage it did not
//! reach, and a run with `--only` is labelled PARTIAL in its last line so
//! it cannot be pasted into a PR as a green gate.
//!
//! An ADVISORY stage (the clone and slop censuses) is an automated mini
//! code review for Claude/agents to act on, not a correctness verdict: a
//! nonzero exit prints REVIEW, the gate stays green and later stages run.
//!
//! Adding a check is ONE entry in [`STAGES`]. If it is not in that list it
//! does not run, and that list is the only place to look to find out what
//! the gate covers. [`SUCCESSORS`] maps every stage of the TS gate it
//! replaces to its successor here (a test holds the mapping complete).

use std::path::Path;
use std::time::{Duration, Instant};

use crate::HostEnv;
use crate::proc::{Spawn, run_streamed};
use crate::settings::Parsed;

/// What a stage runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Run {
    /// `cargo <args>` in the repository root.
    Cargo(&'static [&'static str]),
    /// This binary (`humanify-ag-dev <args>`): an in-crate verb.
    Dev(&'static [&'static str]),
}

/// One gate stage.
#[derive(Clone, Copy, Debug)]
pub struct Stage {
    /// The `--only` selector and the summary label.
    pub name: &'static str,
    /// What it protects: printed when it fails, so the failure is
    /// actionable.
    pub why: &'static str,
    pub run: Run,
    /// Findings are a review for Claude to act on, never a FAIL.
    pub advisory: bool,
    /// The deadline over the whole stage.
    pub deadline: Duration,
}

const MINUTE: Duration = Duration::from_secs(60);

/// Every check, cheapest first so a broken tree fails fast.
pub const STAGES: &[Stage] = &[
    Stage {
        name: "shims",
        why: "the JavaScript that remains (the webcrack shim the pipeline runs for webpack / browserify unpacking) parses under node — the harness TypeScript the old typecheck / lint / knip stages covered is ported to Rust",
        run: Run::Dev(&["shims"]),
        advisory: false,
        deadline: MINUTE,
    },
    Stage {
        name: "no-private-addresses",
        why: "no tracked file holds a private network address — the repo is PUBLIC; docs write <llm-host>, the endpoint lives in the git-ignored .humanify.local.json",
        run: Run::Dev(&["no-private-addresses"]),
        advisory: false,
        deadline: MINUTE,
    },
    Stage {
        name: "rust:fmt",
        why: "rustfmt defaults across crates/ (docs/rust-port/05-rust-toolchain.md §8); formatting drift never reaches review",
        run: Run::Cargo(&["fmt", "--all", "--check"]),
        advisory: false,
        deadline: Duration::from_secs(5 * 60),
    },
    Stage {
        name: "rust:clippy",
        why: "the house rules as lints: complexity ceiling, hash-iteration ban in decision code, env reads confined to their owners, the concurrency bans; warnings fatal at the gate only",
        run: Run::Cargo(&[
            "clippy",
            "--workspace",
            "--all-targets",
            "--",
            "-D",
            "warnings",
        ]),
        advisory: false,
        deadline: Duration::from_secs(60 * 60),
    },
    Stage {
        name: "census:clones",
        why: "ADVISORY — potential duplication across crates/ for Claude to review (an automated mini code review, not a correctness verdict). Act on findings: unify the code or allowlist the twin with a justification",
        run: Run::Dev(&["census", "clones"]),
        advisory: true,
        deadline: Duration::from_secs(10 * 60),
    },
    Stage {
        name: "census:concurrency",
        why: "every concurrency site in non-test crates/ (threads, rayon, tokio, locks, channels, atomics, subprocesses, sleeps, timeouts) is on dev/census/concurrency-allowlist.json with a documented pattern and a reason (docs/concurrency.md); an unlisted or stale site fails — confirm a new one by adding its entry, or use the sanctioned primitive",
        run: Run::Dev(&["census", "concurrency"]),
        advisory: false,
        deadline: Duration::from_secs(10 * 60),
    },
    Stage {
        name: "census:slop",
        why: "ADVISORY — per-crate complexity mass, erosion and clone share against main's merge-base (docs/dev-harness.md §7): a reviewer's prompt, not a verdict",
        run: Run::Dev(&["census", "slop"]),
        advisory: true,
        deadline: Duration::from_secs(10 * 60),
    },
    Stage {
        name: "rust:unit",
        why: "every #[cfg(test)] and tests/ target in crates/ — the pipeline's tests, the frozen test/parity goldens, and the measurement harness's own tests (humanify-ag-dev, humanify-ag-testkit)",
        run: Run::Cargo(&["test", "--workspace"]),
        advisory: false,
        deadline: Duration::from_secs(90 * 60),
    },
    Stage {
        name: "rust:build",
        why: "the RELEASE binary — the pipeline the eval scores and the stages below run. `--locked`: a lockfile drift fails here, not an hour into an eval",
        run: Run::Cargo(&["build", "--release", "--locked", "-p", "humanify-ag-cli"]),
        advisory: false,
        deadline: Duration::from_secs(60 * 60),
    },
    Stage {
        name: "format-golden",
        why: "the release binary's formatter replayed against test/parity/format-goldens.json — the formatter's frozen spec — plus a planted perturbation that must be detected",
        run: Run::Dev(&["format-golden"]),
        advisory: false,
        deadline: Duration::from_secs(10 * 60),
    },
    Stage {
        name: "e2e",
        why: "the release binary end to end on the committed e2e fixtures (fresh + prior, the one stub LLM), deterministic, and every output imported by node with the input's export surface",
        run: Run::Dev(&["e2e"]),
        advisory: false,
        deadline: Duration::from_secs(45 * 60),
    },
];

/// Every stage of the deleted TS gate (`scripts/check.ts` `STAGES`, 13; tag
/// `experiments-code-final` has it) and what
/// replaces it here: a stage name in [`STAGES`], or `MOOT: <why>`.
pub const SUCCESSORS: &[(&str, &str)] = &[
    ("typecheck", "shims"),
    ("lint", "shims"),
    ("rust:fmt", "rust:fmt"),
    ("rust:clippy", "rust:clippy"),
    ("knip", "shims"),
    ("knip:prod", "shims"),
    ("census:clones", "census:clones"),
    ("census:concurrency", "census:concurrency"),
    (
        "unit",
        "MOOT: the harness's *.test.ts are ported into the dev crate's Rust tests, which rust:unit runs",
    ),
    ("rust:unit", "rust:unit"),
    ("rust:build", "rust:build"),
    ("rust:format-golden", "format-golden"),
    ("e2e", "e2e"),
];

/// How one stage came out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Passed,
    Failed,
    Review,
    Skipped,
}

/// One stage's row in the summary.
#[derive(Clone, Debug)]
pub struct Row {
    pub stage: &'static str,
    pub outcome: Outcome,
    pub took: Duration,
}

/// The `--only` selection, checked: every name must be a stage.
pub fn selection(only: &[String]) -> Result<Option<Vec<String>>, String> {
    if only.is_empty() {
        return Ok(None);
    }
    let unknown: Vec<&str> = only
        .iter()
        .map(String::as_str)
        .filter(|n| !STAGES.iter().any(|s| s.name == *n))
        .collect();
    if !unknown.is_empty() {
        let known: Vec<&str> = STAGES.iter().map(|s| s.name).collect();
        return Err(format!(
            "unknown stage(s): {}\nknown: {}",
            unknown.join(", "),
            known.join(", ")
        ));
    }
    Ok(Some(only.to_vec()))
}

/// Run `stages` in order with `exec` (true: the stage exited 0), fail
/// fast on a pass/fail stage's failure. The rows cover every stage that
/// ran or was skipped; a stage after a fail-fast break has no row.
pub fn run_stages(
    stages: &[Stage],
    only: Option<&[String]>,
    exec: &mut dyn FnMut(&Stage) -> bool,
) -> Vec<Row> {
    let mut rows = Vec::new();
    for stage in stages {
        if only.is_some_and(|o| !o.iter().any(|n| n == stage.name)) {
            rows.push(Row {
                stage: stage.name,
                outcome: Outcome::Skipped,
                took: Duration::ZERO,
            });
            continue;
        }
        let started = Instant::now();
        let ok = exec(stage);
        let outcome = match (ok, stage.advisory) {
            (true, _) => Outcome::Passed,
            (false, true) => Outcome::Review,
            (false, false) => Outcome::Failed,
        };
        rows.push(Row {
            stage: stage.name,
            outcome,
            took: started.elapsed(),
        });
        // A later stage's output would bury the failure that matters.
        if outcome == Outcome::Failed {
            break;
        }
    }
    rows
}

/// The summary block and the exit code for `rows` of `stages`.
pub fn summary(stages: &[Stage], rows: &[Row]) -> (u8, String) {
    let rule = "═".repeat(56);
    let mut out = format!("\n{rule}\n");
    for row in rows {
        let (mark, time) = match row.outcome {
            Outcome::Passed => ("PASS", format!("{:.1}s", row.took.as_secs_f64())),
            Outcome::Failed => ("FAIL", format!("{:.1}s", row.took.as_secs_f64())),
            Outcome::Review => ("REVIEW", format!("{:.1}s", row.took.as_secs_f64())),
            Outcome::Skipped => ("skip", String::new()),
        };
        out.push_str(&format!("  {mark:<6} {:<22} {time}\n", row.stage));
    }
    for stage in &stages[rows.len()..] {
        out.push_str(&format!("  ---    {:<22} not reached\n", stage.name));
    }
    let why = |name: &str| stages.iter().find(|s| s.name == name).map_or("", |s| s.why);
    let failed: Vec<&Row> = rows
        .iter()
        .filter(|r| r.outcome == Outcome::Failed)
        .collect();
    if !failed.is_empty() {
        out.push_str(&format!("{rule}\n"));
        for f in &failed {
            out.push_str(&format!("FAILED: {} — {}\n", f.stage, why(f.stage)));
        }
        let unreached = stages.len() - rows.len();
        if unreached > 0 {
            out.push_str(&format!(
                "{unreached} later stage(s) did not run. This is NOT a green gate.\n"
            ));
        }
        return (1, out);
    }
    let reviews: Vec<&Row> = rows
        .iter()
        .filter(|r| r.outcome == Outcome::Review)
        .collect();
    if !reviews.is_empty() {
        out.push_str(&format!("{rule}\n"));
        for r in &reviews {
            out.push_str(&format!("REVIEW: {} — {}\n", r.stage, why(r.stage)));
        }
    }
    let skipped: Vec<&str> = rows
        .iter()
        .filter(|r| r.outcome == Outcome::Skipped)
        .map(|r| r.stage)
        .collect();
    out.push_str(&format!("{rule}\n"));
    if skipped.is_empty() {
        out.push_str(&format!("ALL {} STAGES PASSED\n", stages.len()));
    } else {
        out.push_str(&format!(
            "PARTIAL — {}/{} stages ran (skipped: {}). Not a green gate; run `cargo dev check` with no --only.\n",
            rows.len() - skipped.len(),
            stages.len(),
            skipped.join(", ")
        ));
    }
    (0, out)
}

/// The child process a stage runs.
pub fn spawn_of(stage: &Stage, repo: &Path, host: &HostEnv) -> Result<Spawn, String> {
    let (program, args) = match stage.run {
        Run::Cargo(args) => ("cargo".to_string(), args),
        Run::Dev(args) => (
            host.current_exe
                .as_ref()
                .ok_or("cannot find this binary's own path to run an in-crate stage")?
                .display()
                .to_string(),
            args,
        ),
    };
    Ok(Spawn::new(&program, args, stage.deadline)
        .cwd(repo)
        // The gate owns the env folklore: the user-level cargo and bun,
        // which shells that never sourced ~/.profile (commit hooks, agent
        // tool shells) lack. A missing cargo still fails loudly — it is
        // never skipped.
        .env("PATH", &host.child_path()))
}

/// Run the gate.
pub fn run(p: &Parsed, host: &HostEnv) -> (u8, String, String) {
    let only = match selection(&p.settings.list("dev.only").unwrap_or_default()) {
        Ok(o) => o,
        Err(e) => return (2, String::new(), format!("{e}\n")),
    };
    let repo = crate::paths::repo_root();
    let mut exec = |stage: &Stage| -> bool {
        println!("\n━━━ {} ━━━", stage.name);
        match spawn_of(stage, &repo, host).and_then(|s| run_streamed(&s)) {
            Ok(status) => status.success(),
            Err(e) => {
                eprintln!("{}: {e}", stage.name);
                false
            }
        }
    };
    let rows = run_stages(STAGES, only.as_deref(), &mut exec);
    let (code, text) = summary(STAGES, &rows);
    (code, text, String::new())
}

#[cfg(test)]
#[path = "check_test.rs"]
mod check_test;
