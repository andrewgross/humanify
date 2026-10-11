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
        .arg(&input)
        .args(["--api-key", "k", "--model", "stub-model", "-c", "4"])
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
