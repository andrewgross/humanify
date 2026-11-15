//! `cargo dev census concurrency [--files a.rs,...] [--root D] [--allowlist F]`
//! (successor of scripts/concurrency-census.ts). PASS/FAIL.
//!
//! The pipeline's concurrency approach is written down in
//! docs/concurrency.md: a FIXED set of patterns, each with its rules. This
//! census finds every concurrency construct in non-test Rust under
//! `<root>/crates/` and requires each one to be on the allowlist
//! (dev/census/concurrency-allowlist.json) with the pattern it follows and a
//! reason. An unlisted site FAILS — the "did you really mean this?" flag
//! for whoever added it: the message names the construct, the doc section
//! with its rules, and the exact entry to add once the site is confirmed.
//! A stale entry (its site is gone) also fails, so the allowlist stays an
//! inventory, not a graveyard.
//!
//! A site is (file, scope, construct); see [`scan`] for how scopes are
//! named. Several uses of one construct in one scope are ONE site, and line
//! numbers are not part of the key.
//!
//! Not scanned: test code — `*_test.rs`, anything under a `tests/`
//! directory, a module file declared `#[cfg(test)] mod x;`, and an inline
//! `#[cfg(test)]` item.
//!
//! The allow-comment check: every allow attribute (outer or inner) of a
//! lint in `TAGGED_LINTS` in `crates/` (tests included) carries a
//! `// concurrency: <reason>` comment on its line or in the three lines
//! above — or `// env: <reason>` for the env-reader ban that shares the
//! lint, `// io: <reason>` for an effect ban of a pure crate, `// default:
//! <reason>` for a Result's error turned into a value (the effects census,
//! [`super::effects`], owns those bans; this check serves both).
//!
//! `--files` (the pre-commit hook's staged files): only the named `.rs`
//! files are reported on — their unlisted sites, the stale entries naming
//! them, their allow comments. A list with no `.rs` path is a full run.

pub(crate) mod rules;
pub(crate) mod scan;

use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

use serde::Serialize;

use super::allowlist;
use crate::HostEnv;
use crate::committed::census::{ConcurrencySite, concurrency_allowlist_at};
use crate::paths::CONCURRENCY_ALLOWLIST;
use crate::settings::Parsed;
use rules::{PATTERNS, REQUIRED_PATTERN, TEST_SUPPORT_CRATE, construct};
pub use scan::Site;
use scan::{TestModule, sanitize, sites_of, strip_cfg_test};

const DOC: &str = "docs/concurrency.md";

/// One scanned file.
pub(crate) struct Scanned {
    pub(crate) rel: String,
    pub(crate) raw: String,
    /// Sanitized, `#[cfg(test)]` items blanked.
    pub(crate) text: Vec<u8>,
    /// Root-relative candidate files of its `#[cfg(test)] mod x;`.
    test_modules: Vec<String>,
}

/// Every `.rs` file under `<root>/crates/`, scanned, and which of them are
/// live (non-test) code — shared by the concurrency and effects censuses.
pub(crate) struct Tree {
    pub(crate) files: Vec<Scanned>,
    test_modules: BTreeSet<String>,
}

impl Tree {
    /// Walk and scan `<root>/crates/` (an absent directory is an empty tree).
    pub(crate) fn scan(root: &Path) -> Result<Tree, String> {
        let mut all = Vec::new();
        let crates = root.join("crates");
        if crates.exists() {
            walk_rs(&crates, &mut all)?;
        }
        let files = all
            .iter()
            .map(|abs| scan_file(root, abs))
            .collect::<Result<Vec<_>, _>>()?;
        let test_modules = files
            .iter()
            .flat_map(|s| s.test_modules.iter().cloned())
            .collect();
        Ok(Tree {
            files,
            test_modules,
        })
    }

    /// The non-test files: no test-file name suffix, no `tests` directory,
    /// no module declared `#[cfg(test)] mod x;`.
    pub(crate) fn live(&self) -> Vec<&Scanned> {
        self.files
            .iter()
            .filter(|s| !is_test_path(&s.rel) && !self.test_modules.contains(&s.rel))
            .collect()
    }
}

/// What a census run found (before rendering).
pub struct Findings {
    /// Every site in the non-test files.
    pub sites: Vec<Site>,
    pub unlisted: Vec<Site>,
    pub stale: Vec<ConcurrencySite>,
    pub entry_errors: Vec<String>,
    pub allow_problems: Vec<String>,
    pub files: usize,
    pub live_files: usize,
    pub entries: usize,
    pub exceptions: usize,
}

impl Findings {
    fn failures(&self) -> usize {
        self.unlisted.len() + self.stale.len() + self.entry_errors.len() + self.allow_problems.len()
    }
}

/// Lexically normalize an absolute path (`.` and `..` resolved), as
/// node's `path.resolve`/`path.join` do.
pub(crate) fn normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

/// `path.relative(root, p)` with `/` separators (both normalized).
fn relative(root: &Path, p: &Path) -> String {
    let r: Vec<_> = root.components().collect();
    let q: Vec<_> = p.components().collect();
    let common = r.iter().zip(&q).take_while(|(a, b)| a == b).count();
    let mut parts: Vec<String> = vec!["..".to_string(); r.len() - common];
    parts.extend(
        q[common..]
            .iter()
            .map(|c| c.as_os_str().to_string_lossy().into_owned()),
    );
    parts.join("/")
}

/// Every `.rs` file under `dir` (skipping `target` and `node_modules`;
/// symlinked directories are not followed), sorted by name per directory.
fn walk_rs(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .collect::<Result<_, _>>()
        .map_err(|e| format!("{}: {e}", dir.display()))?;
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let name = e.file_name().to_string_lossy().into_owned();
        let ty = e.file_type().map_err(|err| format!("{name}: {err}"))?;
        if ty.is_dir() {
            if name != "target" && name != "node_modules" {
                walk_rs(&e.path(), out)?;
            }
        } else if name.ends_with(".rs") {
            out.push(e.path());
        }
    }
    Ok(())
}

fn is_test_path(rel: &str) -> bool {
    rel.ends_with("_test.rs") || rel.split('/').any(|c| c == "tests")
}

/// Candidate files for an out-of-line `mod name;` declared in `file`.
fn module_files(file: &Path, m: &TestModule) -> Vec<PathBuf> {
    let dir = file.parent().unwrap_or(Path::new(""));
    if let Some(p) = &m.path_attr {
        return vec![normalize(&dir.join(p))];
    }
    let stem = file
        .file_stem()
        .map_or(String::new(), |s| s.to_string_lossy().into_owned());
    let own = if ["lib", "main", "mod"].contains(&stem.as_str()) {
        dir.to_path_buf()
    } else {
        dir.join(stem)
    };
    vec![
        own.join(format!("{}.rs", m.name)),
        own.join(&m.name).join("mod.rs"),
        dir.join(format!("{}.rs", m.name)),
    ]
}

fn scan_file(root: &Path, abs: &Path) -> Result<Scanned, String> {
    let raw = std::fs::read_to_string(abs).map_err(|e| format!("{}: {e}", abs.display()))?;
    let units: Vec<u16> = raw.encode_utf16().collect();
    let (text, modules) = strip_cfg_test(&sanitize(&raw), &units);
    Ok(Scanned {
        rel: relative(root, abs),
        test_modules: modules
            .iter()
            .flat_map(|m| module_files(abs, m))
            .map(|f| relative(root, &f))
            .collect(),
        raw,
        text,
    })
}

// ─── checks ───────────────────────────────────────────────────────────

/// The lints whose allow says why (enforced: census:concurrency): the concurrency / env / effect
/// bans (`disallowed_*`) and the swallowed-error lints of the pure crates.
const TAGGED_LINTS: &[&str] = &[
    "clippy::disallowed_methods",
    "clippy::disallowed_types",
    "clippy::disallowed_macros",
    "clippy::map_err_ignore",
    "clippy::let_underscore_must_use",
];

/// The reason tags an allow of a [`TAGGED_LINTS`] lint may carry.
const REASON_TAGS: &[&str] = &["concurrency:", "env:", "io:", "default:"];

/// `#!?\[allow\([^\]]*` naming one of [`TAGGED_LINTS`].
fn is_disallowed_allow(line: &str) -> bool {
    ["#[allow(", "#![allow("].iter().any(|open| {
        line.match_indices(open).any(|(at, _)| {
            let rest = &line[at + open.len()..];
            let until = rest.find(']').map_or(rest, |e| &rest[..e]);
            TAGGED_LINTS.iter().any(|l| until.contains(l))
        })
    })
}

/// `\/\/\s*(concurrency|env|io|default):\s*\S`.
fn has_reason(line: &str) -> bool {
    line.match_indices("//").any(|(at, _)| {
        let rest = line[at + 2..].trim_start();
        REASON_TAGS.iter().any(|tag| {
            rest.strip_prefix(tag)
                .is_some_and(|r| r.chars().any(|c| !c.is_whitespace()))
        })
    })
}

/// `#[allow(clippy::disallowed_*)]` lines with no reason comment near.
fn unexplained_allows(f: &Scanned) -> Vec<String> {
    let lines: Vec<&str> = f.raw.split('\n').collect();
    let mut bad = Vec::new();
    for (i, l) in lines.iter().enumerate() {
        // An attribute rustfmt spread over several lines is read whole.
        let attr = (i..lines.len().min(i + 12))
            .find(|&e| lines[e].contains(']'))
            .map_or_else(|| l.to_string(), |e| lines[i..=e].join(" "));
        if !(l.contains("allow(") && is_disallowed_allow(&attr)) {
            continue;
        }
        if lines[i.saturating_sub(3)..=i].iter().any(|n| has_reason(n)) {
            continue;
        }
        bad.push(format!(
            "  {}:{}  {}\n    no `// concurrency: <reason>` comment on this line or the three above\n    (`// env: <reason>` for the env-reader ban that shares the lint; in the pure crates\n    `// io: <reason>` for an effect, `// default: <reason>` for a Result's error made a value).\n    Every allow of a banned method says why this site is the exception ({DOC}#what-clippy-enforces).",
            f.rel,
            i + 1,
            l.trim()
        ));
    }
    bad
}

fn pattern_names() -> String {
    PATTERNS.iter().map(|p| p.0).collect::<Vec<_>>().join(", ")
}

/// What is wrong with an entry's pattern: unknown, or not allowed for its
/// file or construct.
fn pattern_problems(e: &ConcurrencySite, place: &str) -> Vec<String> {
    let mut problems = Vec::new();
    if !e.pattern.is_empty() && !PATTERNS.iter().any(|p| p.0 == e.pattern) {
        problems.push(format!(
            "  {place}: unknown pattern \"{}\" — one of: {} ({DOC}#the-patterns)",
            e.pattern,
            pattern_names()
        ));
    }
    if e.pattern == "test-support" && !e.file.starts_with(TEST_SUPPORT_CRATE) {
        problems.push(format!(
            "  {place}: test-support is only for {TEST_SUPPORT_CRATE} (a test-only crate, never linked into a product binary)"
        ));
    }
    let required = REQUIRED_PATTERN.iter().find(|r| r.0 == e.construct);
    if let Some((_, req)) = required
        && e.pattern != *req
    {
        problems.push(format!(
            "  {place}: {} must use pattern \"{req}\" — {}",
            e.construct,
            construct(&e.construct).map_or("", |c| c.rule)
        ));
    }
    problems
}

fn entry_problems(e: &ConcurrencySite, i: usize) -> Vec<String> {
    let (place, mut problems) = allowlist::basic_problems(
        i,
        &[
            ("file", &e.file),
            ("scope", &e.scope),
            ("construct", &e.construct),
            ("pattern", &e.pattern),
            ("reason", &e.reason),
        ],
        construct(&e.construct).is_some(),
    );
    problems.extend(pattern_problems(e, &place));
    problems
}

/// The entry to paste, in the allowlist's field order.
#[derive(Serialize)]
struct Suggested<'a> {
    file: &'a str,
    scope: &'a str,
    construct: &'a str,
    pattern: String,
    reason: &'a str,
}

fn unlisted_message(s: &Site) -> String {
    let c = construct(&s.construct);
    let entry = Suggested {
        file: &s.file,
        scope: &s.scope,
        construct: &s.construct,
        pattern: format!("<one of: {}>", pattern_names()),
        reason: "<why this site needs it, and how it meets the rule>",
    };
    let json = serde_json::to_string_pretty(&entry).unwrap_or_default();
    let lines: Vec<String> = s.lines.iter().map(ToString::to_string).collect();
    [
        format!(
            "  {}:{}  {}  —  {}",
            s.file,
            lines.join(","),
            s.scope,
            s.construct
        ),
        format!(
            "    Rule ({DOC}#{}): {}",
            c.map_or("", |c| c.section),
            c.map_or("", |c| c.rule)
        ),
        format!("    If this is intended, confirm it: add to {CONCURRENCY_ALLOWLIST}:"),
        json.lines()
            .map(|l| format!("      {l}"))
            .collect::<Vec<_>>()
            .join("\n"),
        format!("    If not, use the sanctioned primitive instead ({DOC})."),
    ]
    .join("\n")
}

/// Run the census over `root` against `entries`; `files` narrows the
/// report to those root-relative paths (None: everything).
pub fn census(
    root: &Path,
    entries: &[ConcurrencySite],
    files: Option<&BTreeSet<String>>,
) -> Result<Findings, String> {
    let tree = Tree::scan(root)?;
    let in_scope = |rel: &str| files.is_none_or(|f| f.contains(rel));

    let allow_problems = tree
        .files
        .iter()
        .filter(|s| in_scope(&s.rel))
        .flat_map(unexplained_allows)
        .collect();
    let live = tree.live();
    let sites: Vec<Site> = live
        .iter()
        .flat_map(|s| sites_of(&s.rel, &s.text))
        .collect();

    let checked = allowlist::check(
        &sites,
        entries,
        |e| (&e.file, &e.scope, &e.construct),
        entry_problems,
        files,
    );
    let (unlisted, stale, entry_errors) = (checked.unlisted, checked.stale, checked.entry_errors);
    Ok(Findings {
        files: tree.files.len(),
        live_files: live.len(),
        entries: entries.len(),
        exceptions: entries.iter().filter(|e| e.exception.is_some()).count(),
        sites,
        unlisted,
        stale,
        entry_errors,
        allow_problems,
    })
}

fn plural_entries(n: usize) -> &'static str {
    if n == 1 { "y" } else { "ies" }
}

/// The report text and its exit code.
fn render(f: &Findings, named: Option<usize>, ms: u128) -> (u8, String) {
    let mut out = String::new();
    allowlist::section(
        &mut out,
        format!(
            "CONCURRENCY CENSUS: {} unlisted concurrency site(s) — confirm each is intended ({DOC})",
            f.unlisted.len()
        ),
        &f.unlisted.iter().map(unlisted_message).collect::<Vec<_>>(),
    );
    allowlist::section(
        &mut out,
        format!(
            "CONCURRENCY CENSUS: {} stale allowlist entr{} — the site is gone (or moved to another function); delete or re-key it in {CONCURRENCY_ALLOWLIST}",
            f.stale.len(),
            plural_entries(f.stale.len())
        ),
        &f.stale
            .iter()
            .map(|e| format!("  {}  {}  —  {}", e.file, e.scope, e.construct))
            .collect::<Vec<_>>(),
    );
    allowlist::section(
        &mut out,
        format!(
            "CONCURRENCY CENSUS: {} malformed allowlist entr{}",
            f.entry_errors.len(),
            plural_entries(f.entry_errors.len())
        ),
        &f.entry_errors,
    );
    allowlist::section(
        &mut out,
        format!(
            "CONCURRENCY CENSUS: {} unexplained clippy allow(s)",
            f.allow_problems.len()
        ),
        &f.allow_problems,
    );
    let scope = named.map_or(format!("{} files", f.files), |n| {
        format!("{n} named file(s)")
    });
    let summary = format!(
        "{} sites in {} non-test files, {} allowlist entries ({} known exceptions), {scope} checked, {ms} ms",
        f.sites.len(),
        f.live_files,
        f.entries,
        f.exceptions
    );
    if f.failures() > 0 {
        out.push_str(&format!("\nCONCURRENCY CENSUS RED — {summary}\n"));
        (1, out)
    } else {
        out.push_str(&format!("CONCURRENCY CENSUS GREEN — {summary}\n"));
        (0, out)
    }
}

/// `--files`: only `.rs` paths narrow the report (resolved against the
/// working directory, then made root-relative); none means a full run.
pub(crate) fn named_files(list: &[String], root: &Path, cwd: &Path) -> Option<BTreeSet<String>> {
    let named: BTreeSet<String> = list
        .iter()
        .filter(|p| p.ends_with(".rs"))
        .map(|p| relative(root, &normalize(&cwd.join(p))))
        .collect();
    (!named.is_empty()).then_some(named)
}

/// Run the verb.
pub fn run(p: &Parsed, host: &HostEnv) -> (u8, String, String) {
    allowlist::run_verb(
        (p, host),
        ("concurrency", CONCURRENCY_ALLOWLIST),
        |f| concurrency_allowlist_at(f).map(|a| a.value.entries),
        |root, entries: &Vec<ConcurrencySite>, files| census(root, entries, files),
        render,
    )
}

#[cfg(test)]
#[path = "concurrency_test.rs"]
mod concurrency_test;
