//! `cargo dev ask-trace <input.js> [--out D] [--prior P] [--collide NAME]
//! [--sequential] [--pipeline-set section.key=value]... [-- <pipeline flags>]`
//! (successor of scripts/ask-trace.ts): the stub side of the
//! `--dump-asks` instrument.
//!
//! Starts the ONE stub LLM (humanify-ag-testkit, in-process — the server
//! the e2e stage runs on) and runs the RELEASE BINARY on any input, cold
//! or `--prior-version`, with `--dump-asks` on. Writes `<out>/asks.jsonl`
//! (one row per LLM ask, reason-labelled) and `<out>/tree/` (the
//! humanified output). Deterministic end to end: the pipeline is
//! completion-order-independent and the stub answers from the request
//! bytes alone, so the same input + flags give the same log byte for byte
//! (`cargo dev diff-asks`'s use (a) depends on exactly this).
//!
//! - `--out D` — the output directory (default: a fresh temp dir);
//! - `--prior P` — run with `--prior-version P`;
//! - `--collide NAME` — the stub names EVERY identifier NAME, forcing the
//!   collision-retry paths (the lane round-2 and the barrier re-ask);
//! - `--sequential` — the conservative naming schedule;
//! - `--pipeline-set section.key=value` — one pipeline setting, checked
//!   against the pipeline's registry and written into the launch's
//!   `--config` file (scripts/ask-trace.ts's `--set`); the split is OFF
//!   unless set, as before exp179 made it the binary's default;
//! - `-- <flags>` — passed to the binary verbatim.
//!
//! Prints the per-reason ask summary (TOTAL first) and exits with the
//! binary's verdict. Its launches are held to lane L0's golden
//! (`launch-goldens/ask-trace.golden.txt`) by `ask_trace_test.rs`. The
//! binary runs in the repository root (the TS inherited the caller's
//! directory; every path it is handed is absolute either way).

use std::path::{Path, PathBuf};
use std::time::Duration;

use humanify_ag_cli::registry::Val;
use humanify_ag_model::js::{JsObject, JsValue, cmp_utf16};
use humanify_ag_testkit::answers;
use humanify_ag_testkit::stub_server::StubServer;

use crate::HostEnv;
use crate::e2e::launch_log::{Setting, setting, write_config};
use crate::proc::{Spawn, run as run_child};
use crate::settings::Parsed;

/// The deadline over one traced run (a whole Claude Code bundle takes
/// minutes; this is a hang guard, not a budget).
const RUN_DEADLINE: Duration = Duration::from_secs(4 * 3600);

const USAGE: &str = "usage: cargo dev ask-trace <input.js> [--out D] [--prior P] [--collide NAME] \
                     [--sequential] [--pipeline-set section.key=value]... [-- <pipeline flags>]";

/// What one trace runs.
#[derive(Debug)]
pub struct AskTraceArgs {
    pub input: PathBuf,
    pub out: Option<PathBuf>,
    pub prior: Option<PathBuf>,
    pub collide: Option<String>,
    pub sequential: bool,
    /// The `--pipeline-set` entries, typed by the pipeline's registry.
    pub settings: Vec<Setting>,
    pub passthrough: Vec<String>,
}

/// A registry value as the `--config` file spells it.
fn js_of(v: &Val) -> JsValue {
    match v {
        Val::Text(s) => JsValue::str(s.as_str()),
        Val::Bool(b) => JsValue::Bool(*b),
        Val::Int(n) => JsValue::Number(*n as f64),
        Val::Float(x) => JsValue::Number(*x),
        Val::List(items) => JsValue::str_array(items),
    }
}

/// The trace's arguments; relative paths resolve against `cwd`. The input
/// is the first positional, and every positional after it is the `--`
/// passthrough (the parser refuses an unknown flag before the `--`).
pub fn args_of(p: &Parsed, cwd: &Path) -> Result<AskTraceArgs, String> {
    let Some((input, passthrough)) = p.positionals.split_first() else {
        return Err(USAGE.into());
    };
    let path = |key: &str| p.settings.text(key).map(|t| cwd.join(t));
    Ok(AskTraceArgs {
        input: cwd.join(input),
        out: path("run.out"),
        prior: path("dev.prior"),
        collide: p.settings.text("dev.collide"),
        sequential: p.settings.flag_on("dev.sequential"),
        settings: p
            .pipeline_sets
            .iter()
            .map(|(k, v)| setting(k, js_of(v)))
            .collect(),
        passthrough: passthrough.to_vec(),
    })
}

/// The per-reason summary, totals first.
pub fn summarize(rows: &[JsObject]) -> String {
    let reason = |r: &JsObject| match r.get("reason") {
        Some(JsValue::String(s)) => s.clone(),
        Some(other) => humanify_ag_model::js::stringify(other),
        None => "undefined".into(),
    };
    let mut counts: Vec<(String, usize)> = Vec::new();
    for r in rows {
        let k = reason(r);
        match counts.iter_mut().find(|(key, _)| *key == k) {
            Some((_, n)) => *n += 1,
            None => counts.push((k, 1)),
        }
    }
    counts.sort_by(|a, b| cmp_utf16(&a.0, &b.0));
    let pad = counts
        .iter()
        .map(|(k, _)| k.encode_utf16().count())
        .max()
        .unwrap_or(0)
        .max("TOTAL".len());
    let retries = rows
        .iter()
        .filter(|r| r.get("isRetry") == Some(&JsValue::Bool(true)))
        .count();
    let mut lines = vec![format!("TOTAL {} ask(s)", rows.len())];
    lines.extend(counts.iter().map(|(k, n)| format!("  {k:<pad$}  {n:>5}")));
    lines.push(format!("  {:<pad$}  {retries:>5} re-ask(s)", "REMAINING"));
    lines.join("\n")
}

/// The launch's `--config` settings: the runner's own, then the user's.
fn launch_settings(out: &Path, a: &AskTraceArgs) -> Vec<Setting> {
    let mut s = vec![
        setting(
            "diagnostics.dump_asks",
            JsValue::str(out.join("asks.jsonl").display().to_string()),
        ),
        setting("llm.retries", JsValue::Number(0.0)),
    ];
    if a.sequential {
        s.push(setting("naming.schedule", JsValue::str("sequential")));
    }
    if !a.settings.iter().any(|(k, _)| k == "split.enabled") {
        s.push(setting("split.enabled", JsValue::Bool(false)));
    }
    s.extend(a.settings.iter().cloned());
    s
}

fn failed(msg: &str) -> (u8, String, String) {
    (1, String::new(), format!("ASK-TRACE FAILED: {msg}\n"))
}

/// The stub for this trace: `<id>Renamed`, or every identifier one name.
fn stub(collide: Option<&str>) -> StubServer {
    match collide {
        Some(name) => {
            let name = name.to_string();
            StubServer::answering(move |body| answers::collide(&name)(body))
        }
        None => StubServer::answering(answers::renamed),
    }
}

/// One trace with the release binary of the checkout at `repo`.
pub fn trace(repo: &Path, a: &AskTraceArgs) -> (u8, String, String) {
    let bin = match crate::e2e::release_binary(repo) {
        Ok(b) => b,
        Err(e) => return failed(&e),
    };
    if !a.input.exists() {
        return failed(&format!("no such input: {}", a.input.display()));
    }
    let out = match &a.out {
        Some(o) => o.clone(),
        None => match crate::e2e::fresh_dir("ask-trace-") {
            Ok(d) => d,
            Err(e) => return failed(&e),
        },
    };
    if let Err(e) = std::fs::create_dir_all(&out) {
        return failed(&format!("{}: {e}", out.display()));
    }
    let config = out.join("humanify.toml");
    if let Err(e) = write_config(&config, &launch_settings(&out, a)) {
        return failed(&e);
    }
    let server = stub(a.collide.as_deref());
    let mut argv: Vec<String> = vec![
        a.input.display().to_string(),
        "-o".into(),
        out.join("tree").display().to_string(),
        "--config".into(),
        config.display().to_string(),
        "--endpoint".into(),
        server.base_url.clone(),
    ];
    if let Some(prior) = &a.prior {
        argv.extend(["--prior-version".into(), prior.display().to_string()]);
    }
    argv.extend(a.passthrough.iter().cloned());
    let mut spawn = Spawn::new(&bin.display().to_string(), &[], RUN_DEADLINE)
        .cwd(repo)
        // The binary's API key comes from HUMANIFY_API_KEY only; the stub
        // accepts any.
        .env("HUMANIFY_API_KEY", "ask-trace");
    spawn.args = argv;
    let (code, child_stderr) = match run_child(&spawn) {
        Ok(done) => (
            done.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&done.stderr).to_string(),
        ),
        Err(e) => return failed(&e),
    };
    let mut stdout = String::new();
    let mut stderr = child_stderr;
    let asks = out.join("asks.jsonl");
    if asks.exists() {
        match crate::e2e::diff_asks::read_asks(&asks) {
            Ok(rows) => stdout.push_str(&format!("{}\n", summarize(&rows))),
            Err(e) => return failed(&e),
        }
    } else {
        stderr.push_str("ASK-TRACE FAILED: the run wrote no ask log\n");
    }
    stdout.push_str(&format!("ask log:  {}\n", asks.display()));
    stdout.push_str(&format!("tree:     {}\n", out.join("tree").display()));
    if code != 0 {
        stderr.push_str(&format!("ASK-TRACE FAILED: the binary exited {code}\n"));
        return (1, stdout, stderr);
    }
    (0, stdout, stderr)
}

/// Run the verb.
pub fn run(p: &Parsed, host: &HostEnv) -> (u8, String, String) {
    let cwd = host.cwd.clone().unwrap_or_else(|| PathBuf::from("."));
    match args_of(p, &cwd) {
        Ok(a) => trace(&crate::e2e::repo(), &a),
        Err(e) => (2, String::new(), format!("{e}\n")),
    }
}

#[cfg(test)]
#[path = "ask_trace_test.rs"]
mod ask_trace_test;
