//! `cargo dev precommit`: the git pre-commit hook (`.githooks/pre-commit`,
//! installed with `git config core.hooksPath .githooks`). The successor of
//! husky + lint-staged + scripts/rustfmt-staged.sh + the TS census's
//! `--files` mode. On the STAGED files:
//!
//! - `*.rs`: rustfmt in place and re-staged (a file that also has
//!   unstaged changes is refused instead: formatting it would stage
//!   them), then the concurrency census over just those files;
//! - `scripts/concurrency-allowlist.json` staged: the whole census;
//! - every staged file's STAGED content: no private network address.
//!
//! The hook is not the gate: it never runs the test suites.
//!
//! Until lane L5 removes the TypeScript harness, husky stays the
//! installed hook (its prettier/biome keep the TS gate's lint stage
//! green); L5 switches `core.hooksPath` to `.githooks`.

use std::path::Path;
use std::time::Duration;

use crate::HostEnv;
use crate::proc::{self, Spawn};
use crate::settings::Parsed;

/// The committed hook script.
pub const HOOK: &str = ".githooks/pre-commit";
const ALLOWLIST: &str = crate::paths::CONCURRENCY_ALLOWLIST;
const DEADLINE: Duration = Duration::from_secs(10 * 60);

/// What the hook does for a set of staged paths.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
    pub rustfmt: Vec<String>,
    /// None: no census. Some(empty): the whole tree. Some(files): those.
    pub census: Option<Vec<String>>,
    pub address_scan: Vec<String>,
}

/// The plan for `staged` (repo-relative paths, added/copied/modified/renamed).
pub fn plan(staged: &[String]) -> Plan {
    let rust: Vec<String> = staged
        .iter()
        .filter(|p| p.ends_with(".rs"))
        .cloned()
        .collect();
    let census = if staged.iter().any(|p| p == ALLOWLIST) {
        Some(Vec::new())
    } else if rust.is_empty() {
        None
    } else {
        Some(rust.clone())
    };
    Plan {
        rustfmt: rust,
        census,
        address_scan: staged.to_vec(),
    }
}

/// This binary's arguments for the census over `files` (empty: all).
pub fn census_args(files: &[String]) -> Vec<String> {
    let mut args = vec!["census".to_string(), "concurrency".to_string()];
    if !files.is_empty() {
        args.push("--files".into());
        args.push(files.join(","));
    }
    args
}

fn lines(text: &str) -> Vec<String> {
    text.split('\0')
        .filter(|p| !p.is_empty())
        .map(str::to_string)
        .collect()
}

/// rustfmt `files` in place and re-stage them; refuse a file with
/// unstaged changes.
fn format_staged(repo: &Path, files: &[String], host: &HostEnv) -> Result<(), String> {
    if files.is_empty() {
        return Ok(());
    }
    let mut args = vec!["diff", "--name-only", "-z", "--"];
    args.extend(files.iter().map(String::as_str));
    let dirty = lines(&proc::git(repo, &args)?);
    if !dirty.is_empty() {
        return Err(format!(
            "these staged .rs files also have UNSTAGED changes; stage them or run `cargo fmt --all` and stage the result: {}",
            dirty.join(", ")
        ));
    }
    let mut spawn = Spawn::new("rustfmt", &[], DEADLINE)
        .cwd(repo)
        .env("PATH", &host.child_path());
    spawn.args.extend(files.iter().cloned());
    proc::run_ok(&spawn)?;
    let mut add = vec!["add", "--"];
    add.extend(files.iter().map(String::as_str));
    proc::git(repo, &add).map(|_| ())
}

fn census(repo: &Path, files: &[String], host: &HostEnv) -> Result<(), String> {
    let exe = host
        .current_exe
        .as_ref()
        .ok_or("cannot find this binary's own path")?;
    let mut spawn = Spawn::new(&exe.display().to_string(), &[], DEADLINE)
        .cwd(repo)
        .env("PATH", &host.child_path());
    spawn.args = census_args(files);
    let done = proc::run(&spawn)?;
    if done.status.success() {
        return Ok(());
    }
    Err(format!(
        "{}{}",
        String::from_utf8_lossy(&done.stdout),
        String::from_utf8_lossy(&done.stderr)
    ))
}

/// The private addresses in the STAGED content of `files`.
fn scan_staged(repo: &Path, files: &[String]) -> Result<Vec<String>, String> {
    let mut staged = Vec::new();
    for f in files {
        let spawn = Spawn::new(
            "git",
            &["-C", &repo.display().to_string(), "show", &format!(":{f}")],
            DEADLINE,
        );
        let done = proc::run(&spawn)?;
        if done.status.success() {
            staged.push((f.clone(), done.stdout));
        }
    }
    Ok(super::private_addresses::scan(
        &staged,
        super::private_addresses::ALLOWED,
    ))
}

/// Run the hook on this repository's staged files.
pub fn run(_p: &Parsed, host: &HostEnv) -> (u8, String, String) {
    let repo = match proc::git(
        &crate::paths::repo_root(),
        &["rev-parse", "--show-toplevel"],
    ) {
        Ok(r) => std::path::PathBuf::from(r),
        Err(e) => return (1, String::new(), format!("precommit: {e}\n")),
    };
    let staged = match proc::git(
        &repo,
        &[
            "diff",
            "--cached",
            "--name-only",
            "--diff-filter=ACMR",
            "-z",
        ],
    ) {
        Ok(s) => lines(&s),
        Err(e) => return (1, String::new(), format!("precommit: {e}\n")),
    };
    let plan = plan(&staged);
    if let Err(e) = format_staged(&repo, &plan.rustfmt, host) {
        return (1, String::new(), format!("precommit (rustfmt): {e}\n"));
    }
    if let Some(files) = &plan.census
        && let Err(e) = census(&repo, files, host)
    {
        return (
            1,
            String::new(),
            format!("precommit (concurrency census):\n{e}"),
        );
    }
    match scan_staged(&repo, &plan.address_scan) {
        Ok(hits) if hits.is_empty() => (
            0,
            format!("precommit: {} staged file(s) ok\n", staged.len()),
            String::new(),
        ),
        Ok(hits) => (1, String::new(), super::private_addresses::report(&hits)),
        Err(e) => (1, String::new(), format!("precommit: {e}\n")),
    }
}

#[cfg(test)]
#[path = "precommit_test.rs"]
mod precommit_test;
