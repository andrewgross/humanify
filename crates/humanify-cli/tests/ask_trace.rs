//! The ask trace (`--dump-asks`) end to end: the stub-named collision
//! fixture of the 2026-09-28 fix, read through the INSTRUMENT instead of
//! the model — the re-ask appears with its recorded cause, is bounded at
//! `reask::REASK_LIMIT`, and the whole log is deterministic run to run
//! (the pipeline is completion-order-independent by design and the stub
//! answers deterministically).

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

/// The deterministic stub's answer: `q1` and `e0` both suggest
/// `eventHooks` (the collision); everything else gets `<id>Named`.
fn answer(user: &str) -> String {
    let ids = user
        .lines()
        .find_map(|l| l.strip_prefix("Identifiers to rename: "))
        .unwrap_or("");
    let entries: serde_json::Map<String, Value> = ids
        .split(", ")
        .filter(|s| !s.is_empty())
        .map(|id| {
            let name = if id == "q1" || id == "e0" {
                "eventHooks".to_string()
            } else {
                format!("{id}Named")
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

fn run(dir: &Path, input: &str, out: &Path, asks: &Path, endpoint: &str) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_humanify"))
        .current_dir(dir)
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
        .arg(asks)
        .output()
        .unwrap()
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
    let url = start_stub();
    let asks = s.0.join(format!("asks-{run_i}.jsonl"));
    let out = s.0.join(format!("out-{run_i}"));
    let result = run(&s.0, input, &out, &asks, &url);
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
    // BOUNDED at reask::REASK_LIMIT — no scope re-asks twice.
    let mut scopes: Vec<String> = retries
        .iter()
        .map(|r| field(r, "scope").as_str().unwrap_or_default().to_string())
        .collect();
    scopes.sort();
    let mut bounded = scopes.clone();
    bounded.dedup();
    assert_eq!(
        scopes.len(),
        bounded.len(),
        "one re-ask per scope (REASK_LIMIT=1)"
    );
}
