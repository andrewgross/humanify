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

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

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

fn serve(stream: std::net::TcpStream, counter: &AtomicUsize, seen: &Mutex<Vec<String>>) {
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
    let n = counter.fetch_add(1, Ordering::SeqCst);
    // Earlier requests answer later: completion order is not dispatch order.
    std::thread::sleep(std::time::Duration::from_millis(((7 - n % 7) * 15) as u64));
    let v: Value = serde_json::from_slice(&body).unwrap();
    let user = v["messages"][1]["content"].as_str().unwrap_or("");
    seen.lock().unwrap().push(user.to_string());
    let out = serde_json::json!({
        "choices": [{"message": {"role": "assistant", "content": answer(user, n)},
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

type Seen = Arc<Mutex<Vec<String>>>;

/// Start the stub; returns its base URL, its request counter and the user
/// prompts it was sent.
fn start_recording() -> (String, Arc<AtomicUsize>, Seen) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/v1", listener.local_addr().unwrap());
    let counter = Arc::new(AtomicUsize::new(0));
    let seen: Seen = Arc::default();
    let (c, s) = (counter.clone(), seen.clone());
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let (c, s) = (c.clone(), s.clone());
            std::thread::spawn(move || serve(stream, &c, &s));
        }
    });
    (url, counter, seen)
}

/// Start the stub; returns its base URL and its request counter.
fn start() -> (String, Arc<AtomicUsize>) {
    let (url, counter, _) = start_recording();
    (url, counter)
}

fn run(dir: &Path, input: &str, out: &Path, cache: &Path, endpoint: &str) -> Output {
    command(dir, input, out, endpoint)
        .arg("--llm-cache")
        .arg(cache)
        .output()
        .unwrap()
}

fn command(dir: &Path, input: &str, out: &Path, endpoint: &str) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_humanify"));
    cmd.current_dir(dir)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .args([input, "--api-key", "k", "--model", "stub-model", "-c", "8"])
        .args(["--retries", "0", "--endpoint", endpoint])
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
    let (url, counter) = start();

    let live = run(&s.0, &input, &s.0.join("live"), &cache, &url);
    let err = String::from_utf8_lossy(&live.stderr).into_owned();
    assert_eq!(live.status.code(), Some(0), "{err}");
    assert!(
        counter.load(Ordering::SeqCst) > 0,
        "the live run asked the stub"
    );
    let filled = cache_entries(&cache);

    // The replay's endpoint is a second stub that must never be asked.
    let (replay_url, replay_counter) = start();
    let replay = run(&s.0, &input, &s.0.join("replay"), &cache, &replay_url);
    let err = String::from_utf8_lossy(&replay.stderr).into_owned();
    assert_eq!(replay.status.code(), Some(0), "{err}");
    assert_eq!(
        replay_counter.load(Ordering::SeqCst),
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
    let (url, counter, seen) = start_recording();

    let out = command(&s.0, &input, &s.0.join("out"), &url)
        .output()
        .unwrap();
    let err = String::from_utf8_lossy(&out.stderr).into_owned();
    assert_eq!(out.status.code(), Some(0), "{err}");
    let prompts = seen.lock().unwrap().clone();
    assert_eq!(prompts.len(), counter.load(Ordering::SeqCst));
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
