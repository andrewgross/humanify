//! A failed LLM call is a hole in the output: the run carries on without
//! those names and still exits 0 (the fail-soft decision, unchanged). That
//! decision must not be SILENT — before the concurrency audit (2026-10,
//! R2) the error count was printed only under `-v`, so an endpoint outage
//! mid-run shipped a degraded tree with no default-level trace.

use std::path::PathBuf;
use std::process::Command;

const INPUT: &str = concat!(
    "var dA=(H,_)=>{let q=Object.assign(H);_.forEach($=>$.configure(q));return Object.assign(H,q)};",
    "module.exports={dA};\n"
);

struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// An endpoint nothing listens on (bound, then released).
fn dead_endpoint() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);
    format!("http://{addr}/v1")
}

#[test]
fn provider_errors_are_reported_at_the_default_verbosity() {
    let dir = std::env::temp_dir().join(format!("humanify-provider-errors-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let s = Scratch(dir);
    let input = s.0.join("bundle.js");
    std::fs::write(&input, INPUT).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_humanify"))
        .current_dir(&s.0)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HUMANIFY_API_KEY", "k")
        .arg(&input)
        .args(["--model", "stub-model", "-c", "4"])
        .args(["--retries", "0", "--endpoint", &dead_endpoint()])
        .arg("-o")
        .arg(s.0.join("out"))
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    // The fail-soft decision itself is unchanged: the run completes.
    assert_eq!(out.status.code(), Some(0), "{text}");
    let warning = text
        .lines()
        .find(|l| l.contains("WARNING") && l.contains("LLM call(s) failed"))
        .unwrap_or_else(|| panic!("no provider-error WARNING at the default verbosity:\n{text}"));
    assert!(
        warning.contains("names are missing"),
        "the warning says what it costs: {warning}"
    );
}

/// A stub endpoint that fails EVERY call with a 503 the client is told not
/// to retry (`x-should-retry: false`), so each call is one request: the
/// fault-injection half of audit recommendation 1c. Returns its base URL
/// and its request counter.
fn failing_endpoint() -> (String, std::sync::Arc<std::sync::atomic::AtomicUsize>) {
    use std::io::{BufRead, BufReader, Read, Write};
    use std::sync::atomic::Ordering;
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/v1", listener.local_addr().unwrap());
    let count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let seen = count.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let seen = seen.clone();
            std::thread::spawn(move || {
                let mut reader = BufReader::new(stream);
                let mut length = 0usize;
                let mut line = String::new();
                while reader.read_line(&mut line).unwrap_or(0) > 0 {
                    if line == "\r\n" {
                        break;
                    }
                    if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        length = v.trim().parse().unwrap_or(0);
                    }
                    line.clear();
                }
                let mut body = vec![0u8; length];
                let _ = reader.read_exact(&mut body);
                seen.fetch_add(1, Ordering::SeqCst);
                let out = r#"{"error":{"message":"injected fault"}}"#;
                let _ = write!(
                    reader.get_mut(),
                    "HTTP/1.1 503 Service Unavailable\r\ncontent-type: application/json\r\n\
                     x-should-retry: false\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{out}",
                    out.len()
                );
            });
        }
    });
    (url, count)
}

/// What one run against the failing endpoint reported: the
/// `providerErrors` block, the endpoint's request count, the output.
struct FailedRun {
    provider_errors: serde_json::Value,
    requests: usize,
    text: String,
}

fn run_failing(fixture: &str) -> FailedRun {
    use std::sync::atomic::Ordering;
    let dir = std::env::temp_dir().join(format!(
        "humanify-provider-stats-{fixture}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let s = Scratch(dir);
    let input = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
        "../../test/e2e/fixtures/{fixture}/build/v1.0.0/build/index.js"
    ));
    let stats = s.0.join("stats.json");
    let (endpoint, requests) = failing_endpoint();
    let out = Command::new(env!("CARGO_BIN_EXE_humanify"))
        .current_dir(&s.0)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HUMANIFY_API_KEY", "k")
        .arg(&input)
        .args(["--split", "--model", "stub-model", "-c", "16"])
        .args(["--retries", "0", "--endpoint", &endpoint])
        .arg("--stats-json")
        .arg(&stats)
        .arg("-o")
        .arg(s.0.join("out"))
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    // The fail-soft decision is unchanged: the run completes, exit 0.
    assert_eq!(out.status.code(), Some(0), "{text}");
    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&stats).unwrap()).unwrap();
    FailedRun {
        provider_errors: json["providerErrors"].clone(),
        requests: requests.load(Ordering::SeqCst),
        text,
    }
}

/// The checks every failing run must pass: the total is the sum of the
/// namers AND exactly the requests the endpoint failed (no failed call
/// goes uncounted), each listed namer asked and failed, and ONE WARNING
/// carries the total. Returns the block.
fn assert_counted(run: &FailedRun, namers: &[&str]) {
    let p = &run.provider_errors;
    let n = |k: &str| {
        p[k].as_u64()
            .unwrap_or_else(|| panic!("providerErrors.{k}: {p}"))
    };
    let total = n("total");
    assert_eq!(
        total,
        n("naming") + n("modules") + n("vendor") + n("split") + n("treeReviser"),
        "{p}"
    );
    for namer in namers {
        assert!(n(namer) > 0, "{namer} asked and failed: {p}");
    }
    assert_eq!(
        total as usize, run.requests,
        "every request the endpoint failed is one counted call: {p}"
    );
    let warnings: Vec<&str> = run
        .text
        .lines()
        .filter(|l| l.contains("WARNING") && l.contains("LLM call(s) failed"))
        .collect();
    assert_eq!(
        warnings.len(),
        1,
        "one WARNING for every namer:\n{}",
        run.text
    );
    assert!(
        warnings[0].starts_with(&format!("WARNING: {total} LLM call(s) failed")),
        "{}",
        warnings[0]
    );
}

/// A Bun app with vendor code and a fresh-grouping split: the waves and
/// sweeps, the vendor namer, the split namer and the tree reviser all ask,
/// and all fail (audit R2: the last three were stats-only or silent).
#[test]
fn every_namers_failed_calls_reach_the_stats_on_a_fresh_grouping_split() {
    assert_counted(
        &run_failing("bun-mixed"),
        &["naming", "vendor", "split", "treeReviser"],
    );
}

/// A Bun app whose lazy modules the module namer names: its failed calls
/// were only in the "Module names:" line (audit R2, "still not covered").
#[test]
fn the_module_namers_failed_calls_reach_the_stats() {
    assert_counted(&run_failing("bun-lazy"), &["naming", "vendor", "modules"]);
}
