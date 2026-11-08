//! `cargo dev format-golden`: the release binary's formatter
//! (`core::format`, pipeline stage 6) replayed against the committed
//! goldens — test/parity/format-goldens.json, captured from the TS
//! beautifier before the cutover and now the formatter's FROZEN SPEC
//! (test/parity/README.md). The successor of scripts/format-golden.sh.
//!
//! Two runs, because a check that cannot fail proves nothing:
//!   1. every golden case must format to its recorded bytes;
//!   2. the same check with a planted perturbation must FAIL, detected as
//!      a differing case — proof the comparison is live.
//!
//! Needs target/release/humanify-ag (the gate's rust:build stage builds it).

use std::path::Path;
use std::time::Duration;

use crate::HostEnv;
use crate::proc::{self, Spawn};
use crate::settings::Parsed;

/// The release binary, relative to the repository root.
pub const RELEASE_BIN: &str = "target/release/humanify-ag";
const GOLDENS: &str = "test/parity/format-goldens.json";
const PLANT: &str = "rust-number-format";
const DEADLINE: Duration = Duration::from_secs(5 * 60);

/// The check, then the planted check.
pub fn launches(repo: &Path) -> [Spawn; 2] {
    let bin = repo.join(RELEASE_BIN).display().to_string();
    let goldens = repo.join(GOLDENS).display().to_string();
    [
        Spawn::new(&bin, &["format-check", &goldens], DEADLINE),
        Spawn::new(
            &bin,
            &["format-check", &goldens, "--plant", PLANT, "--show", "0"],
            DEADLINE,
        ),
    ]
}

/// Whether the planted run proves the check live: it failed AND reported
/// at least one differing case.
pub fn plant_detected(exited_ok: bool, output: &str) -> Result<(), String> {
    let differing = output.split("differing ").skip(1).any(|rest| {
        rest.chars()
            .next()
            .is_some_and(|c| c.is_ascii_digit() && c != '0')
    });
    if exited_ok || !differing {
        let tail: Vec<&str> = output.lines().rev().take(3).collect();
        let tail: Vec<&str> = tail.into_iter().rev().collect();
        return Err(format!(
            "FORMAT GOLDENS: a planted perturbation was NOT detected — the check is vacuous:\n{}\n",
            tail.join("\n")
        ));
    }
    Ok(())
}

/// Run both checks.
pub fn run(_p: &Parsed, _host: &HostEnv) -> (u8, String, String) {
    let repo = crate::paths::repo_root();
    if !repo.join(RELEASE_BIN).is_file() {
        return (
            1,
            String::new(),
            format!(
                "no binary at {RELEASE_BIN} — build it: cargo build --release --locked -p humanify-ag-cli (the gate's rust:build stage)\n"
            ),
        );
    }
    let [check, plant] = launches(&repo);
    let mut out = String::new();
    match run_child(&check) {
        Ok((true, text)) => out.push_str(&text),
        Ok((false, text)) => {
            return (
                1,
                text,
                "FORMAT GOLDENS: the formatter drifted from its frozen spec\n".into(),
            );
        }
        Err(e) => return (1, String::new(), format!("{e}\n")),
    }
    match run_child(&plant) {
        Ok((ok, text)) => match plant_detected(ok, &text) {
            Ok(()) => {
                out.push_str("  planted perturbation detected (the check can fail)\n");
                (0, out, String::new())
            }
            Err(e) => (1, out, e),
        },
        Err(e) => (1, out, format!("{e}\n")),
    }
}

/// Exit status and stdout + stderr of one child.
fn run_child(spawn: &Spawn) -> Result<(bool, String), String> {
    let done = proc::run(spawn)?;
    let mut text = String::from_utf8_lossy(&done.stdout).to_string();
    text.push_str(&String::from_utf8_lossy(&done.stderr));
    Ok((done.status.success(), text))
}

#[cfg(test)]
#[path = "format_golden_test.rs"]
mod format_golden_test;
