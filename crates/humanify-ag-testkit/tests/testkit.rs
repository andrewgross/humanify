//! The one stub server, its answer policies and the shuffled fake.

use std::collections::BTreeSet;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::Arc;
use std::time::Duration;

use humanify_ag_model::llm::{BatchRenameRequest, LlmCall, NameProvider};
use humanify_ag_testkit::answers::{asked_identifiers, collide, renamed, user_prompt};
use humanify_ag_testkit::fake::{ShuffledFake, names, reply};
use humanify_ag_testkit::stub_server::{StubResponse, StubServer, dead_endpoint};

fn chat(ids: &str) -> String {
    serde_json::json!({
        "model": "m",
        "messages": [
            {"role": "system", "content": "sys"},
            {"role": "user", "content": format!("code\nIdentifiers to rename: {ids}\nmore")}
        ]
    })
    .to_string()
}

/// POST `body` to the stub over one fresh connection; the raw response.
fn post(base_url: &str, body: &str) -> String {
    let addr = base_url
        .trim_start_matches("http://")
        .trim_end_matches("/v1");
    let mut s = TcpStream::connect(addr).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    write!(
        s,
        "POST /v1/chat/completions HTTP/1.1\r\nhost: x\r\ncontent-type: application/json\r\nx-probe: 1\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    )
    .unwrap();
    let mut out = String::new();
    // The server keeps the connection open (keep-alive): read the head,
    // then exactly content-length bytes.
    let mut buf = [0u8; 4096];
    loop {
        let n = s.read(&mut buf).unwrap_or(0);
        if n == 0 {
            break;
        }
        out.push_str(&String::from_utf8_lossy(&buf[..n]));
        if let Some((head, rest)) = out.split_once("\r\n\r\n") {
            let len: usize = head
                .lines()
                .find_map(|l| l.strip_prefix("content-length: "))
                .and_then(|v| v.trim().parse().ok())
                .unwrap_or(0);
            if rest.len() >= len {
                break;
            }
        }
    }
    out
}

#[test]
fn the_default_policy_renames_every_asked_identifier() {
    let body = chat("a, $b, 9bad, c_1");
    assert_eq!(asked_identifiers(&body), ["a", "$b", "9bad", "c_1"]);
    let answer: serde_json::Value = serde_json::from_str(&renamed(&body)).unwrap();
    assert_eq!(
        answer,
        serde_json::json!({"a": "aRenamed", "$b": "$bRenamed", "c_1": "c_1Renamed"}),
        "a non-identifier is skipped"
    );
    let collided: serde_json::Value = serde_json::from_str(&collide("same")(&body)).unwrap();
    assert_eq!(collided["a"], "same");
    assert_eq!(collided["c_1"], "same");
}

#[test]
fn a_module_batch_gets_stable_four_word_names() {
    let body = serde_json::json!({"messages": [{"role": "user", "content": "files:\n{\"m1\": \"<file-name>\", \"m2\": \"<file-name>\"}"}]}).to_string();
    let a: serde_json::Value = serde_json::from_str(&renamed(&body)).unwrap();
    let b: serde_json::Value = serde_json::from_str(&renamed(&body)).unwrap();
    assert_eq!(a, b, "a repeat asks the same");
    for key in ["m1", "m2"] {
        assert_eq!(a[key].as_str().unwrap().split('-').count(), 4, "{a}");
    }
}

#[test]
fn the_stub_answers_and_records_each_request() {
    let stub = StubServer::answering(renamed);
    let raw = post(&stub.base_url, &chat("x"));
    assert!(raw.starts_with("HTTP/1.1 200"), "{raw}");
    let body = raw.split_once("\r\n\r\n").unwrap().1;
    let v: serde_json::Value = serde_json::from_str(body).unwrap();
    let content = v["choices"][0]["message"]["content"].as_str().unwrap();
    assert_eq!(content, r#"{"x":"xRenamed"}"#);
    assert_eq!(stub.requests(), 1);
    assert_eq!(stub.bodies(), vec![chat("x")]);
    assert!(stub.headers()[0].contains(&("x-probe".to_string(), "1".to_string())));
}

#[test]
fn the_stub_answers_a_status_with_headers_or_hangs_up() {
    let stub = StubServer::start(
        Duration::ZERO,
        Arc::new(|i, _| match i {
            0 => StubResponse::status(429, "{}").with_header("retry-after", "1"),
            _ => StubResponse::hang_up(),
        }),
    );
    let raw = post(&stub.base_url, "{}");
    assert!(
        raw.starts_with("HTTP/1.1 429") && raw.contains("retry-after: 1"),
        "{raw}"
    );
    assert_eq!(post(&stub.base_url, "{}"), "", "a hang-up answers nothing");
}

#[test]
fn the_stub_measures_peak_concurrency() {
    let stub = Arc::new(StubServer::start(
        Duration::from_millis(200),
        Arc::new(|_, _| StubResponse::ok("{}")),
    ));
    let handles: Vec<_> = (0..4)
        .map(|_| {
            let url = stub.base_url.clone();
            std::thread::spawn(move || post(&url, "{}"))
        })
        .collect();
    for h in handles {
        assert!(h.join().unwrap().starts_with("HTTP/1.1 200"));
    }
    assert_eq!(stub.requests(), 4);
    assert!(
        stub.max_in_flight() >= 2,
        "{} in flight at most",
        stub.max_in_flight()
    );
}

fn call(id: &str) -> LlmCall {
    LlmCall {
        request: BatchRenameRequest {
            identifiers: vec![id.to_string()],
            ..Default::default()
        },
        system_prompt: String::new(),
        user_prompt: String::new(),
    }
}

/// Completion order: reproducible per seed, different across seeds, and
/// never simply id order — the order the real provider is free to use.
#[test]
fn the_fake_completes_in_a_seeded_shuffled_order() {
    let run = |seed| {
        let fake = ShuffledFake::renaming(seed);
        let initial: Vec<(usize, LlmCall)> = (0..8).map(|i| (i, call(&format!("v{i}")))).collect();
        let mut seen = Vec::new();
        fake.run_pipelined(initial, &mut |id, r| {
            let name = r
                .unwrap()
                .renames
                .get(&format!("v{id}"))
                .map(str::to_string);
            seen.push((id, name));
            // Every first-generation call starts one follow-up.
            if id < 8 {
                vec![(id + 100, call(&format!("v{}", id + 100)))]
            } else {
                vec![]
            }
        });
        (fake.completion_order(), seen)
    };
    let (a, seen) = run(1);
    assert_eq!(a, run(1).0, "same seed, same order");
    assert_ne!(a, run(2).0, "another seed, another order");
    let ids: BTreeSet<usize> = a.iter().copied().collect();
    assert_eq!(ids.len(), 16, "every call and follow-up completed once");
    let mut sorted = a.clone();
    sorted.sort();
    assert_ne!(a, sorted, "not id order");
    assert!(
        seen.iter()
            .all(|(id, n)| n.as_deref() == Some(format!("v{id}Renamed").as_str()))
    );
}

#[test]
fn the_fakes_wave_keeps_call_order() {
    let fake = ShuffledFake::renaming(7);
    let out = fake.run_wave(vec![call("a"), call("b")]);
    let names: Vec<Option<String>> = out
        .into_iter()
        .zip(["a", "b"])
        .map(|(r, id)| r.unwrap().renames.get(id).map(str::to_string))
        .collect();
    assert_eq!(
        names,
        [Some("aRenamed".to_string()), Some("bRenamed".to_string())]
    );
}

/// A per-response delay (added after the handler answers): a later
/// request can finish first — completion order is not arrival order.
#[test]
fn a_response_can_carry_its_own_delay() {
    let stub = Arc::new(StubServer::start(
        Duration::ZERO,
        Arc::new(|i, _| {
            let wait = if i == 0 { 400 } else { 0 };
            StubResponse::ok(format!("{i}")).after(Duration::from_millis(wait))
        }),
    ));
    let (tx, rx) = std::sync::mpsc::channel();
    let first = {
        let (url, tx) = (stub.base_url.clone(), tx.clone());
        std::thread::spawn(move || tx.send(post(&url, "{}")).unwrap())
    };
    // Let the first request arrive (index 0) before the second.
    while stub.requests() == 0 {
        std::thread::yield_now();
    }
    let second = {
        let url = stub.base_url.clone();
        std::thread::spawn(move || tx.send(post(&url, "{}")).unwrap())
    };
    first.join().unwrap();
    second.join().unwrap();
    let order: Vec<String> = rx
        .iter()
        .map(|raw| raw.split_once("\r\n\r\n").unwrap().1.to_string())
        .collect();
    assert_eq!(order, ["1", "0"], "the delayed first request finished last");
}

/// A response that sends its head and part of its body, then stalls
/// with the connection open (a half-open link, a wedged proxy).
#[test]
fn a_response_can_stall_after_its_head() {
    let stub = StubServer::start(
        Duration::ZERO,
        Arc::new(|_, _| StubResponse::stalled(1000, "{\"choi")),
    );
    let addr = stub
        .base_url
        .trim_start_matches("http://")
        .trim_end_matches("/v1");
    let mut s = TcpStream::connect(addr).unwrap();
    s.set_read_timeout(Some(Duration::from_millis(500)))
        .unwrap();
    write!(
        s,
        "POST /v1/chat/completions HTTP/1.1\r\nhost: x\r\ncontent-length: 2\r\n\r\n{{}}"
    )
    .unwrap();
    let mut got = String::new();
    let mut buf = [0u8; 4096];
    let timed_out = loop {
        match s.read(&mut buf) {
            Ok(0) => break false,
            Ok(n) => got.push_str(&String::from_utf8_lossy(&buf[..n])),
            Err(_) => break true,
        }
    };
    assert!(timed_out, "the connection stays open: {got}");
    assert!(
        got.starts_with("HTTP/1.1 200") && got.contains("content-length: 1000"),
        "{got}"
    );
    assert!(got.ends_with("\r\n\r\n{\"choi"), "{got}");
}

/// An endpoint nothing listens on: a connection to it is refused.
#[test]
fn a_dead_endpoint_refuses_connections() {
    let url = dead_endpoint();
    let addr = url.trim_start_matches("http://").trim_end_matches("/v1");
    assert!(TcpStream::connect(addr).is_err(), "{url}");
}

#[test]
fn the_user_prompt_is_the_last_message() {
    assert_eq!(
        user_prompt(&chat("a")),
        "code\nIdentifiers to rename: a\nmore"
    );
    assert_eq!(user_prompt("not json"), "");
}

/// `answering` takes a plain closure; `calls` records every call the fake
/// answered, in answer order (waves and pipelined alike); `reply` and
/// `names` build responses.
#[test]
fn the_fake_records_its_calls_and_builds_replies() {
    let fake = ShuffledFake::answering(3, |c| {
        if c.request.identifiers == ["skip"] {
            return reply([("skip", None)]);
        }
        names(c, |id| format!("{id}X"))
    });
    let out = fake.run_wave(vec![call("a"), call("skip")]);
    assert_eq!(out[0].as_ref().unwrap().renames.get("a"), Some("aX"));
    assert_eq!(out[1].as_ref().unwrap().renames.get("skip"), None);
    let mut done = Vec::new();
    fake.run_pipelined(
        (0..4).map(|i| (i, call(&format!("p{i}")))).collect(),
        &mut |id, _| {
            done.push(id);
            vec![]
        },
    );
    let asked: Vec<String> = fake
        .calls()
        .iter()
        .map(|c| c.request.identifiers[0].clone())
        .collect();
    let mut expect = vec!["a".to_string(), "skip".to_string()];
    expect.extend(done.iter().map(|i| format!("p{i}")));
    assert_eq!(asked, expect, "answer order, the shuffled half included");
}
