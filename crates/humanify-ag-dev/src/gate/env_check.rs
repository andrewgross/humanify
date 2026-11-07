//! `cargo dev env-check`: can this machine run the gates? Run it before
//! claiming a work package and before citing any Rust gate
//! (docs/rust-port/RUNBOOK.md §1). The successor of
//! scripts/rust-env-check.sh: the toolchain pin, clippy and rustfmt, the
//! MSRV equal to the pin, node (the webcrack shim and the e2e import
//! check) with the shim's node_modules, bun (boot gates), the corpus
//! bundles under pairs.json `inputsBase`, /work; and, when an endpoint
//! resolves (`--endpoint`, else .humanify.local.json), whether the LLM
//! answers — information only, never a failure.

use std::path::Path;
use std::time::Duration;

use crate::HostEnv;
use crate::proc::{self, Spawn};
use crate::settings::Parsed;

/// One line of the report.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    pub what: String,
    pub said: String,
    pub failed: bool,
}

impl Line {
    pub fn ok(what: &str, said: &str) -> Line {
        Line {
            what: what.into(),
            said: said.into(),
            failed: false,
        }
    }
    /// Information: never fails the check.
    pub fn info(what: &str, said: &str) -> Line {
        Line::ok(what, said)
    }
    pub fn fail(what: &str, said: &str) -> Line {
        Line {
            what: what.into(),
            said: format!("FAIL — {said}"),
            failed: true,
        }
    }
}

/// `channel = "<pin>"` of rust-toolchain.toml.
pub fn pin(toolchain: &str) -> Option<String> {
    quoted_value(toolchain, "channel = ")
}

/// `rust-version = "<msrv>"` of the workspace Cargo.toml.
pub fn msrv(cargo_toml: &str) -> Option<String> {
    quoted_value(cargo_toml, "rust-version = ")
}

/// The first line starting `prefix"<v>"`: v. (Line by line, like
/// bin_build: TOML reading stays with the pipeline's config owner.)
fn quoted_value(text: &str, prefix: &str) -> Option<String> {
    text.lines()
        .find_map(|l| l.strip_prefix(prefix)?.strip_prefix('"')?.split('"').next())
        .map(str::to_string)
}

/// The report text and exit code.
pub fn report(lines: &[Line]) -> (u8, String) {
    let mut out = String::new();
    for l in lines {
        out.push_str(&format!("{:<28} {}\n", l.what, l.said));
    }
    if lines.iter().any(|l| l.failed) {
        out.push_str("RUST ENV NOT OK\n");
        (1, out)
    } else {
        out.push_str("RUST ENV OK\n");
        (0, out)
    }
}

const DEADLINE: Duration = Duration::from_secs(120);

/// stdout of `program args` run in `repo` with the user tools on PATH.
fn tool(repo: &Path, host: &HostEnv, program: &str, args: &[&str]) -> Option<String> {
    let spawn = Spawn::new(program, args, DEADLINE)
        .cwd(repo)
        .env("PATH", &host.child_path());
    proc::run_ok(&spawn).ok().map(|s| s.trim().to_string())
}

fn toolchain_lines(repo: &Path, host: &HostEnv) -> Vec<Line> {
    let read = |f: &str| std::fs::read_to_string(repo.join(f)).unwrap_or_default();
    let Some(pinned) = pin(&read("rust-toolchain.toml")) else {
        return vec![Line::fail("rust-toolchain.toml", "no channel pin")];
    };
    let mut lines = Vec::new();
    match tool(repo, host, "rustc", &["--version"]) {
        None => lines.push(Line::fail(
            "rustc",
            "not on PATH. Install rustup (--profile minimal --component clippy --component rustfmt), then export PATH=\"$HOME/.cargo/bin:$PATH\"",
        )),
        Some(v) => {
            let v = v.split_whitespace().nth(1).unwrap_or_default().to_string();
            lines.push(if v == pinned {
                Line::ok("rustc", &format!("{v} (= pin)"))
            } else {
                Line::fail(
                    "rustc",
                    &format!("{v} but rust-toolchain.toml pins {pinned} (run `cargo --version` in the repo root: rustup installs the pin)"),
                )
            });
        }
    }
    for c in ["clippy", "fmt"] {
        lines.push(match tool(repo, host, "cargo", &[c, "--version"]) {
            Some(_) => Line::ok(&format!("cargo {c}"), "ok"),
            None => Line::fail(
                &format!("cargo {c}"),
                &format!("component missing: rustup component add {c} (fmt = rustfmt)"),
            ),
        });
    }
    let m = msrv(&read("Cargo.toml")).unwrap_or_default();
    lines.push(if m == pinned {
        Line::ok("Cargo.toml rust-version", &format!("{m} (= pin)"))
    } else {
        Line::fail("Cargo.toml rust-version", &format!("{m} != pin {pinned}"))
    });
    lines.push(match tool(repo, host, "cargo-nextest", &["--version"]) {
        Some(_) => Line::info("cargo-nextest", "present (rust:unit may switch to it)"),
        None => Line::info("cargo-nextest", "absent (rust:unit uses cargo test)"),
    });
    lines
}

fn host_lines(repo: &Path, host: &HostEnv) -> Vec<Line> {
    let mut lines = Vec::new();
    lines.push(match tool(repo, host, "bun", &["--version"]) {
        Some(v) => Line::ok("bun", &format!("{v} (boot gates)")),
        None => Line::fail("bun", "absent — boot gates cannot run (~/.bun/bin)"),
    });
    lines.push(match tool(repo, host, "node", &["--version"]) {
        Some(v) => Line::ok(
            "node",
            &format!("{v} (the webcrack shim, e2e's import check)"),
        ),
        None => Line::fail(
            "node",
            "absent — the webcrack shim and the e2e import check need it",
        ),
    });
    lines.push(if repo.join("node_modules/webcrack").is_dir() {
        Line::ok("node_modules", "present (webcrack)")
    } else {
        Line::fail(
            "node_modules",
            "webcrack missing — run: npm ci --ignore-scripts",
        )
    });
    lines.push(match crate::committed::eval::pairs(repo) {
        Err(e) => Line::fail("corpus bundles", &e),
        Ok(p) => {
            let base = Path::new(&p.value.inputs_base);
            let n = std::fs::read_dir(base)
                .map(|d| {
                    d.filter_map(Result::ok)
                        .filter(|e| {
                            e.file_name()
                                .to_string_lossy()
                                .starts_with("claude-code-2.1.")
                        })
                        .count()
                })
                .unwrap_or(0);
            if n > 0 {
                Line::ok(
                    "corpus bundles",
                    &format!("{n} versions under {}", base.display()),
                )
            } else {
                Line::fail("corpus bundles", &format!("none under {}", base.display()))
            }
        }
    });
    lines.push(if Path::new("/work").is_dir() {
        Line::ok("/work", "present (the eval workdir)")
    } else {
        Line::fail("/work", "missing — the eval workdir")
    });
    lines.push(if Path::new("/work/neutrality-cache").is_dir() {
        Line::info("neutrality cache", "present")
    } else {
        Line::info(
            "neutrality cache",
            "absent (a warm cache is captured by the first neutrality run)",
        )
    });
    lines
}

/// The LLM line: information only.
fn llm_line(repo: &Path, host: &HostEnv, flag: Option<&str>) -> Line {
    let Ok(endpoint) = crate::committed::local::resolve_endpoint(flag, repo) else {
        return Line::info(
            "llm",
            "skipped — no endpoint configured (fine for the gate; needed for cold evals)",
        );
    };
    let models = format!("{}/models", endpoint.trim_end_matches('/'));
    match tool(repo, host, "curl", &["-s", "-m", "3", &models]) {
        Some(body) => {
            let id = serde_json::from_str::<serde_json::Value>(&body)
                .ok()
                .and_then(|v| v["data"][0]["id"].as_str().map(str::to_string));
            match id {
                Some(id) => Line::info("llm", &format!("answers, model {id}")),
                None => Line::info(
                    "llm",
                    "unreachable or not OpenAI-compatible (needed for cold evals)",
                ),
            }
        }
        None => Line::info("llm", "unreachable (needed for cold evals)"),
    }
}

/// Run every check. The endpoint itself is never printed (the repo is
/// public and logs get pasted).
pub fn run(p: &Parsed, host: &HostEnv) -> (u8, String, String) {
    let repo = crate::paths::repo_root();
    let mut lines = toolchain_lines(&repo, host);
    lines.extend(host_lines(&repo, host));
    lines.push(llm_line(
        &repo,
        host,
        p.settings.text("llm.endpoint").as_deref(),
    ));
    let (code, text) = report(&lines);
    (code, text, String::new())
}

#[cfg(test)]
#[path = "env_check_test.rs"]
mod env_check_test;
