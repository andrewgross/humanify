//! The ask trace (`--dump-asks`) end to end: the stub-named collision
//! fixture of the 2026-09-28 fix, read through the INSTRUMENT instead of
//! the model — the re-asks appear with their recorded cause, are bounded
//! at `reask::REASK_LIMIT` (2 since 2026-09-29), and the whole log is
//! deterministic run to run (the pipeline is completion-order-independent
//! by design and the stub answers deterministically).

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

/// Two module-level nodes whose stub suggestions collide on `eventHooks`
/// (the module-collision fixture of processor_test): the barrier rejects
/// the loser and it gets the ONE disclosed re-ask.
const INPUT: &str = concat!(
    "var q1 = 1;\n",
    "var q2 = 2;\n",
    "function e0(p) {\n  return p + q1;\n}\n",
    "console.log(e0(q2), q1, q2);\n"
);

/// The ask-trace reason taxonomy (`naming::ask_trace`), verbatim — the log
/// may never grow a reason outside it.
const REASONS: [&str; 8] = [
    "fresh",
    "prior-hinted",
    "shadowed",
    "retry",
    "module-lane",
    "sweep",
    "vendor",
    "folders",
];

struct Scratch(PathBuf);

impl Scratch {
    fn new(tag: &str) -> Scratch {
        let dir = std::env::temp_dir().join(format!("humanify-asks-{tag}-{}", std::process::id()));
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

/// The deterministic stub's answer, keyed on the prompt's own
/// disclosure: a first-round ask suggests `eventHooks` for the colliding
/// pair (`q1` and `e0`), a retry with ONE failed suggestion disclosed
/// (re-ask 1) offers the sibling's applied name `q2Named` — also taken —
/// and a retry with the ACCUMULATED history (re-ask 2, two disclosed
/// failures) offers `eventHooks` again. Every answer is a collision, so
/// the loser runs the re-ask budget out, and the re-ask series only
/// reaches round 2 if round 1's prompt actually disclosed its failure.
fn answer(user: &str) -> String {
    let ids = user
        .lines()
        .find_map(|l| {
            l.strip_prefix("Identifiers to rename: ")
                .or_else(|| l.strip_prefix("Identifiers still needing names: "))
        })
        .unwrap_or("");
    let disclosed = user.matches(" was suggested as ").count();
    let entries: serde_json::Map<String, Value> = ids
        .split(", ")
        .filter(|s| !s.is_empty())
        .map(|id| {
            let name = if disclosed == 0 {
                if id == "q1" || id == "e0" {
                    "eventHooks".to_string()
                } else {
                    format!("{id}Named")
                }
            } else if disclosed == 1 {
                "q2Named".to_string()
            } else {
                "eventHooks".to_string()
            };
            (id.to_string(), Value::String(name))
        })
        .collect();
    Value::Object(entries).to_string()
}

fn serve(stream: std::net::TcpStream) {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut len = 0usize;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap_or(0) == 0 {
            return;
        }
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        if let Some((k, v)) = line.split_once(':')
            && k.eq_ignore_ascii_case("content-length")
        {
            len = v.trim().parse().unwrap();
        }
    }
    let mut body = vec![0u8; len];
    reader.read_exact(&mut body).unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    let user = v["messages"][1]["content"].as_str().unwrap_or("");
    let out = serde_json::json!({
        "choices": [{"message": {"role": "assistant", "content": answer(user)},
                     "finish_reason": "stop"}],
        "usage": {"prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2}
    })
    .to_string();
    let mut stream = stream;
    let _ = write!(
        stream,
        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\n\
         content-length: {}\r\nconnection: close\r\n\r\n{out}",
        out.len()
    );
}

/// The stub on an ephemeral port, answering every request deterministically.
fn start_stub() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/v1", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            std::thread::spawn(move || serve(stream));
        }
    });
    url
}

fn run(
    dir: &Path,
    input: &str,
    out: &Path,
    asks: &Path,
    endpoint: &str,
    extra: &[&str],
) -> std::process::Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_humanify"));
    cmd.current_dir(dir)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .args([
            input,
            "--api-key",
            "k",
            "--model",
            "stub-model",
            "--retries",
            "0",
        ])
        .arg("--endpoint")
        .arg(endpoint)
        .arg("-o")
        .arg(out)
        .arg("--dump-asks")
        .arg(asks);
    for a in extra {
        cmd.arg(a);
    }
    cmd.output().unwrap()
}

/// The parsed ask rows, in recording order.
fn rows(asks: &Path) -> Vec<Value> {
    let text = std::fs::read_to_string(asks).unwrap();
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("a JSONL row"))
        .collect()
}

/// One instrumented run of the collision fixture; returns the log's bytes.
fn run_once(s: &Scratch, input: &str, run_i: usize) -> String {
    run_with(s, input, run_i, &[])
}

/// [`run_once`] with extra CLI args.
fn run_with(s: &Scratch, input: &str, run_i: usize, extra: &[&str]) -> String {
    let url = start_stub();
    let asks = s.0.join(format!("asks-{run_i}.jsonl"));
    let out = s.0.join(format!("out-{run_i}"));
    let result = run(&s.0, input, &out, &asks, &url, extra);
    let err = String::from_utf8_lossy(&result.stderr).into_owned();
    assert_eq!(result.status.code(), Some(0), "{err}");
    assert!(asks.exists(), "the flag wrote the log: {err}");
    std::fs::read_to_string(&asks).unwrap()
}

fn field(r: &Value, k: &str) -> Value {
    r[k].clone()
}

/// The ask log is deterministic run to run: the same input and the same
/// deterministic answers produce the same log byte for byte (the
/// comparator's use (a) — a no-op change diffs to EMPTY — needs the log
/// itself stable first).
#[test]
fn the_ask_log_is_deterministic_run_to_run() {
    let s = Scratch::new("determinism");
    let input = s.0.join("bundle.js");
    std::fs::write(&input, INPUT).unwrap();
    let input = input.display().to_string();
    let first = run_once(&s, &input, 0);
    let second = run_once(&s, &input, 1);
    assert_eq!(first, second, "the ask log must be deterministic");
}

/// The collision fixture read through the INSTRUMENT: every row inside the
/// taxonomy and carrying the full schema; the re-ask recorded with its
/// cause (reask.rs's NameTaken), bounded (one per scope), and every
/// re-asked scope asked in the first round — no silently-missing retry.
#[test]
fn the_collision_reask_is_recorded_and_bounded() {
    let s = Scratch::new("collision");
    let input = s.0.join("bundle.js");
    std::fs::write(&input, INPUT).unwrap();
    let input = input.display().to_string();
    run_once(&s, &input, 0);

    let rows = rows(&s.0.join("asks-0.jsonl"));
    assert!(!rows.is_empty(), "the run asked the stub");
    for r in &rows {
        let reason = field(r, "reason").as_str().unwrap_or_default().to_string();
        assert!(
            REASONS.contains(&reason.as_str()),
            "the taxonomy is closed: {reason} ({r})"
        );
        for key in [
            "seq",
            "site",
            "scope",
            "scopeKind",
            "reason",
            "isRetry",
            "priorContext",
            "round",
            "identifiers",
            "usedNamesCount",
            "promptVariant",
        ] {
            assert!(r.get(key).is_some(), "row carries {key}: {r}");
        }
    }

    // The collision re-ask: reason=retry, the recorded cause (the
    // barrier's used-set collision → reask.rs's NameTaken).
    let retries: Vec<&Value> = rows
        .iter()
        .filter(|r| field(r, "reason") == "retry")
        .collect();
    assert!(
        !retries.is_empty(),
        "the fixture's collision produced a re-ask: {}",
        serde_json::to_string(&rows).unwrap()
    );
    for r in &retries {
        assert_eq!(
            field(r, "retryCause"),
            "NameTaken",
            "the recorded cause: {r}"
        );
        let scope = field(r, "scope").as_str().unwrap_or_default().to_string();
        assert!(
            rows.iter()
                .any(|x| field(x, "scope") == scope && field(x, "reason") != "retry"),
            "the re-asked scope {scope} was asked in the first round"
        );
    }
    // BOUNDED at reask::REASK_LIMIT (2): no scope re-asks more than twice,
    // and the fixture's stubborn loser (the stub's answers all collide)
    // runs the budget to exhaustion — exactly two.
    let mut scopes: Vec<String> = retries
        .iter()
        .map(|r| field(r, "scope").as_str().unwrap_or_default().to_string())
        .collect();
    scopes.sort();
    let mut bounded = scopes.clone();
    bounded.dedup();
    assert_eq!(
        scopes.len(),
        2 * bounded.len(),
        "exactly two re-asks per re-asked scope (REASK_LIMIT=2)"
    );
}

/// The ACCUMULATED disclosure, end to end through the binary: the second
/// re-ask's prompt names EVERY prior suggestion and why it was rejected
/// (read from `--dump-artifacts` prompts.jsonl), and the ask log's
/// per-scope `round` is the cumulative attempt number (initial 1, re-ask
/// one 2, re-ask two 3).
#[test]
fn the_second_reask_prompt_carries_the_accumulated_blocklist() {
    let s = Scratch::new("accumulated");
    let input = s.0.join("bundle.js");
    std::fs::write(&input, INPUT).unwrap();
    let input = input.display().to_string();
    let url = start_stub();
    let asks = s.0.join("asks.jsonl");
    let out = s.0.join("out");
    let artifacts = s.0.join("artifacts");
    let result = run(
        &s.0,
        &input,
        &out,
        &asks,
        &url,
        &["--dump-artifacts", artifacts.to_str().unwrap()],
    );
    let err = String::from_utf8_lossy(&result.stderr).into_owned();
    assert_eq!(result.status.code(), Some(0), "{err}");

    // Two disclosed re-asks of the loser's scope, in recording order.
    let log = rows(&asks);
    let mut retries: Vec<&Value> = log
        .iter()
        .filter(|r| field(r, "reason") == "retry")
        .collect();
    assert_eq!(retries.len(), 2, "the default budget's two re-asks");
    retries.sort_by_key(|r| field(r, "seq").as_u64().unwrap_or_default());
    assert_eq!(
        field(retries[0], "round"),
        Value::from(2u64),
        "the first re-ask is the loser scope's second ask: {}",
        serde_json::to_string(retries[0]).unwrap()
    );
    assert_eq!(
        field(retries[1], "round"),
        Value::from(3u64),
        "the second re-ask is the loser scope's third ask (the cumulative attempt number): {}",
        serde_json::to_string(retries[1]).unwrap()
    );
    let scope = field(retries[0], "scope").as_str().unwrap().to_string();
    assert_eq!(
        field(retries[1], "scope").as_str(),
        Some(scope.as_str()),
        "both re-asks retry the same colliding scope"
    );

    // The prompts: the second re-ask discloses the whole history.
    let prompts_text =
        std::fs::read_to_string(artifacts.join("prompts.jsonl")).expect("prompts.jsonl");
    let retry_prompts: Vec<Value> = prompts_text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("a JSONL row"))
        .filter(|r: &Value| r["site"] == "naming" && r["isRetry"] == Value::Bool(true))
        .collect();
    assert_eq!(retry_prompts.len(), 2, "both re-asks recorded");
    let second = &retry_prompts[1];
    let user = second["userPrompt"].as_str().unwrap();
    let id = second["identifiers"][0].as_str().unwrap();
    for failed in ["eventHooks", "q2Named"] {
        assert!(
            user.contains(&format!(
                "- \"{id}\" was suggested as \"{failed}\" but that conflicts with an existing name"
            )),
            "the {failed} failure disclosed: {user}"
        );
    }
    assert!(
        user.contains("DO NOT suggest these names: eventHooks, q2Named"),
        "the accumulated do-not-suggest block: {user}"
    );
    // The budget exhausted: the loser keeps the deterministic decoration.
    // (Without --split the passthrough adapter's `<out>/index.js` is
    // rewritten in place with the shipped text.)
    let shipped = std::fs::read_to_string(out.join("index.js")).expect("the shipped text");
    assert!(
        shipped.contains("eventHooksVal"),
        "the suffix ladder settled the loser: {shipped}"
    );
}

/// `--rename-retries <n>` sizes the budget: the default gives the
/// collision loser TWO disclosed re-asks, `1` restores the old single
/// re-ask, and `0` never re-asks — the colliding suggestion falls straight
/// to the deterministic suffix ladder and the run still exits clean.
#[test]
fn the_rename_retries_flag_sizes_the_reask_budget() {
    let s = Scratch::new("flag");
    let input = s.0.join("bundle.js");
    std::fs::write(&input, INPUT).unwrap();
    let input = input.display().to_string();

    let count = |run_i: usize, extra: &[&str]| {
        let url = start_stub();
        let asks = s.0.join(format!("asks-{run_i}.jsonl"));
        let out = s.0.join(format!("out-{run_i}"));
        let result = run(&s.0, &input, &out, &asks, &url, extra);
        assert_eq!(
            result.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        rows(&asks)
            .into_iter()
            .filter(|r| field(r, "reason") == "retry")
            .count()
    };
    assert_eq!(count(0, &[]), 2, "the default budget is TWO re-asks");
    assert_eq!(
        count(1, &["--rename-retries", "1"]),
        1,
        "--rename-retries 1 restores the old single re-ask"
    );
    assert_eq!(
        count(2, &["--rename-retries", "0"]),
        0,
        "--rename-retries 0 never re-asks"
    );
    // Disabled re-asks still resolve the collision: the deterministic
    // decoration lands in the passthrough adapter's `<out>/index.js` and
    // the run exits 0 (asserted inside `count`).
    let shipped = std::fs::read_to_string(s.0.join("out-2/index.js")).expect("the shipped text");
    assert!(
        shipped.contains("eventHooksVal"),
        "the suffix ladder settles the loser without any re-ask: {shipped}"
    );
}
