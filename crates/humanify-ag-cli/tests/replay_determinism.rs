//! Finding #57 end to end: a LIVE run fills a scratch cache, and a REPLAY
//! of that cache (same binary, same input, dead endpoint) must ask exactly
//! the questions the live run asked — zero misses, zero writes — and write
//! a byte-identical tree. The stub model answers every request it sees
//! with a DIFFERENT name and a varying delay, so any request sent live
//! twice gets two answers and completes out of order.
//!
//! The input carries the shape that broke it on 2.1.85→86: the same
//! extension-configuration arrow in several (AWS) clients, so one naming
//! wave holds the same request more than once.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::Arc;
use std::time::Duration;

use humanify_ag_testkit::answers::{completion, user_prompt};
use humanify_ag_testkit::stub_server::{StubResponse, StubServer};
use serde_json::Value;

const INPUT: &str = concat!(
    "var dA=(H,_)=>{let q=Object.assign(H);_.forEach($=>$.configure(q));return Object.assign(H,q)};",
    "var dB=(H,_)=>{let q=Object.assign(H);_.forEach($=>$.configure(q));return Object.assign(H,q)};",
    "var dC=(H,_)=>{let q=Object.assign(H);_.forEach($=>$.configure(q));return Object.assign(H,q)};",
    "module.exports={dA,dB,dC};\n"
);

struct Scratch(PathBuf);

impl Scratch {
    fn new(tag: &str) -> Scratch {
        let dir =
            std::env::temp_dir().join(format!("humanify-replay-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Scratch(dir)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// `{"<id>": "v<n>x<i>", …}` for the prompt's "Identifiers to rename:"
/// line — every request numbered, so no two requests agree.
fn answer(user: &str, n: usize) -> String {
    let ids = user
        .lines()
        .find_map(|l| l.strip_prefix("Identifiers to rename: "))
        .unwrap_or("");
    let entries: serde_json::Map<String, Value> = ids
        .split(", ")
        .filter(|s| !s.is_empty())
        .enumerate()
        .map(|(i, id)| (id.to_string(), Value::String(format!("v{n}x{i}"))))
        .collect();
    Value::Object(entries).to_string()
}

/// The user prompts the stub was sent, in arrival order.
fn prompts(stub: &StubServer) -> Vec<String> {
    stub.bodies().iter().map(|b| user_prompt(b)).collect()
}

/// Start the stub: request `n` (arrival order) is answered `answer(_, n)`
/// after `(7 - n % 7) * 15` ms — earlier requests answer later, so
/// completion order is not dispatch order.
fn start() -> StubServer {
    StubServer::start(
        Duration::ZERO,
        Arc::new(|n, body| {
            let wait = Duration::from_millis(((7 - n % 7) * 15) as u64);
            StubResponse::ok(completion(Some(&answer(&user_prompt(body), n)))).after(wait)
        }),
    )
}

fn run(dir: &Path, input: &str, out: &Path, cache: &Path, endpoint: &str) -> Output {
    command(
        dir,
        input,
        out,
        endpoint,
        &format!("llm.cache_dir = {cache:?}\n"),
    )
    .output()
    .unwrap()
}

/// One launch; `settings` are extra `--config` lines (dotted TOML).
/// Library matching off: names (the default) needs a library index since
/// exp174, and this test is about replay. Split off: the default since
/// exp179 is on, and this test was written for the one-file run.
fn command(dir: &Path, input: &str, out: &Path, endpoint: &str, settings: &str) -> Command {
    let config = out.with_extension("toml");
    std::fs::write(
        &config,
        format!(
            "llm.concurrency = 8\nllm.retries = 0\nlibrary.matching = \"off\"\n\
             split.enabled = false\n{settings}"
        ),
    )
    .unwrap();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_humanify-ag"));
    cmd.current_dir(dir)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HUMANIFY_API_KEY", "k")
        .args([input, "--model", "stub-model", "--endpoint", endpoint])
        .arg("--config")
        .arg(&config)
        .arg("-o")
        .arg(out);
    cmd
}

/// Every file under `root`, relative path → bytes, sorted.
fn tree(root: &Path) -> Vec<(String, Vec<u8>)> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, Vec<u8>)>) {
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(root, &p, out);
            } else {
                let rel = p.strip_prefix(root).unwrap().display().to_string();
                out.push((rel, std::fs::read(&p).unwrap()));
            }
        }
    }
    let mut out = Vec::new();
    walk(root, root, &mut out);
    out.sort();
    out
}

fn cache_entries(cache: &Path) -> usize {
    tree(cache).len()
}

#[test]
fn a_replay_of_a_live_run_asks_nothing_new_and_writes_the_same_tree() {
    let s = Scratch::new("live-replay");
    let input = s.0.join("bundle.js");
    std::fs::write(&input, INPUT).unwrap();
    let input = input.display().to_string();
    let cache = s.0.join("cache");
    let stub = start();
    let url = stub.base_url.clone();

    let live = run(&s.0, &input, &s.0.join("live"), &cache, &url);
    let err = String::from_utf8_lossy(&live.stderr).into_owned();
    assert_eq!(live.status.code(), Some(0), "{err}");
    assert!(stub.requests() > 0, "the live run asked the stub");
    let filled = cache_entries(&cache);

    // The replay's endpoint is a second stub that must never be asked.
    let replay_stub = start();
    let replay_url = replay_stub.base_url.clone();
    let replay = run(&s.0, &input, &s.0.join("replay"), &cache, &replay_url);
    let err = String::from_utf8_lossy(&replay.stderr).into_owned();
    assert_eq!(replay.status.code(), Some(0), "{err}");
    assert_eq!(
        replay_stub.requests(),
        0,
        "the replay asked a question the live run never did"
    );
    assert_eq!(cache_entries(&cache), filled, "the replay wrote nothing");
    let live_tree = tree(&s.0.join("live"));
    assert!(!live_tree.is_empty());
    assert!(
        live_tree == tree(&s.0.join("replay")),
        "the replay wrote a different tree"
    );
}

/// Rule 9's pin (docs/concurrency.md, the ordered mode; exp163's ordering
/// audit): the ordered TEST mode is a SCHEDULE, not a behaviour — the turn
/// driver, the sequential `par` helpers and the one-call dispatch windows
/// must produce the conservative schedule's bytes exactly. A live
/// conservative-schedule run fills a scratch cache; an ORDERED replay of
/// that cache must ask nothing new (zero misses, zero writes) and write
/// the byte-identical tree.
///
/// The mode forces the conservative schedule because the obvious wider
/// design — ordered mode ORTHOGONAL to the tier (a relaxed tier + turn
/// driver) — was tried here first and REFUTED by this test's own shape:
/// the turn driver asks 3 questions on this input that the pipelined
/// driver never did (the drivers' prompts differ under the relaxed
/// levers), so it is not the same bytes. The mode therefore admits proven
/// territory only (`FastTier::Off` ≡ `Exact`, the e2e legacy goldens).
///
/// Diagnosis record for the pin itself (the `--dump-asks` +
/// `--dump-artifacts` run that settled it, 2026-10-30): an earlier edit of
/// this test dropped the replay leg's `llm.cache_dir`, which read as "3
/// questions the conservative run never did". With the cache line back,
/// the truth is the GOOD case: the two drivers' ASK STREAMS differ only by
/// ONE EXTRA ask under the turn driver (7 vs 6 here — the pipelined
/// driver's `step_lane` skips a `$` lane's ask that the turn driver still
/// makes), but every cache key the ordered run computes is a key the
/// conservative run also computed (3 distinct keys, byte-equal seq for
/// seq), so the replay is COMPLETE: zero misses, zero writes, byte-identical
/// tree. The key-SET identity is the contract this test pins — the ask
/// COUNT is dispatch detail, like the timing gauges.
#[test]
fn the_ordered_mode_writes_the_conservative_schedule_s_tree() {
    let s = Scratch::new("ordered");
    let input = s.0.join("bundle.js");
    std::fs::write(&input, INPUT).unwrap();
    let input = input.display().to_string();
    let cache = s.0.join("cache");
    let stub = start();
    let url = stub.base_url.clone();

    // The live leg: the CONSERVATIVE schedule (naming.schedule =
    // "sequential", the config sweep's spelling of the old --sequential).
    let live = command(
        &s.0,
        &input,
        &s.0.join("live"),
        &url,
        &format!(
            "llm.cache_dir = {:?}\nnaming.schedule = \"sequential\"\n",
            cache
        ),
    )
    .output()
    .unwrap();
    let err = String::from_utf8_lossy(&live.stderr).into_owned();
    assert_eq!(live.status.code(), Some(0), "{err}");
    assert!(stub.requests() > 0, "the live run asked the stub");
    let filled = cache_entries(&cache);
    let live_tree = tree(&s.0.join("live"));
    assert!(!live_tree.is_empty());

    // The replay's endpoint is a stub that must never be asked.
    let replay_stub = start();
    let replay_url = replay_stub.base_url.clone();
    let out = s.0.join("ordered");
    let replay = command(
        &s.0,
        &input,
        &out,
        &replay_url,
        &format!("llm.cache_dir = {:?}\nrun.order = \"ordered\"\n", cache),
    )
    .output()
    .unwrap();
    let err = String::from_utf8_lossy(&replay.stderr).into_owned();
    assert_eq!(replay.status.code(), Some(0), "{err}");
    assert_eq!(
        replay_stub.requests(),
        0,
        "the ordered replay asked a question the conservative run never did"
    );
    assert_eq!(
        cache_entries(&cache),
        filled,
        "the replay wrote cache entries"
    );
    assert!(
        live_tree == tree(&out),
        "the ordered mode wrote a different tree than the conservative schedule"
    );
}

/// The same guarantee WITHOUT `--llm-cache` — how production runs, the cold
/// eval and the version walks run. The copies of the configure arrow are
/// one request: the model is asked it ONCE, and the run says how many
/// copies were served that answer.
#[test]
fn without_a_cache_identical_requests_reach_the_model_once() {
    let s = Scratch::new("no-cache");
    let input = s.0.join("bundle.js");
    std::fs::write(&input, INPUT).unwrap();
    let input = input.display().to_string();
    let stub = start();
    let url = stub.base_url.clone();

    let out = command(&s.0, &input, &s.0.join("out"), &url, "")
        .output()
        .unwrap();
    let err = String::from_utf8_lossy(&out.stderr).into_owned();
    assert_eq!(out.status.code(), Some(0), "{err}");
    let prompts = prompts(&stub);
    assert_eq!(prompts.len(), stub.requests());
    let mut distinct = prompts.clone();
    distinct.sort();
    distinct.dedup();
    assert_eq!(
        distinct.len(),
        prompts.len(),
        "a prompt reached the model more than once"
    );
    let line = err
        .lines()
        .find(|l| l.contains("LLM requests: "))
        .unwrap_or_else(|| panic!("no memo line in: {err}"));
    eprintln!("{line}");
    assert!(
        line.contains(&format!(
            "LLM requests: {} sent to the model, ",
            prompts.len()
        )),
        "{line}"
    );
    assert!(
        !line.contains(", 0 identical copies"),
        "the configure arrow's copies were shared: {line}"
    );
}
