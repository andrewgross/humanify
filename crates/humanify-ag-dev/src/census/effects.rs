//! `cargo dev census effects [--files <staged .rs>] [--root D] [--allowlist F]`.
//! PASS/FAIL.
//!
//! Functional core, imperative shell: `humanify-ag-core` and
//! `humanify-ag-model` decide over values; disk, processes, the
//! environment, clocks, threads, terminal output and the network belong to
//! the shell crates (cli, index, llm). This census finds every effect
//! construct in the non-test sources of the product crates, every global
//! mutable `static`, and every non-private fn returning `Result<_, String>`,
//! and requires each one to be on dev/census/effects-allowlist.json with
//! its role:
//!
//! - `legacy`: an effect in a pure crate, or a `Result<_, String>`, that a
//!   migration step removes. The list only shrinks: a stale entry (its
//!   site is gone) fails, so each step deletes what it migrated (enforced: census:effects);
//! - `observation`: the profiling clock and recorder (its files only);
//! - `shell`: an effect in a shell crate, an inventory.
//!
//! An unlisted site fails. Keys are (file, scope, construct) named by the
//! concurrency census's scanner; line numbers are not part of a key.
//!
//! It also holds the static layer of the same rule in place:
//!
//! - each pure crate's `clippy.toml` contains every entry of the root
//!   `clippy.toml` (clippy reads the NEAREST config and does not merge it
//!   with the root one, so a root ban missing from a crate config does not
//!   apply there), and the pure crates' configs agree;
//! - each pure crate's `src/lib.rs` keeps the swallowed-error deny line;
//! - each pure crate's normal dependencies stay within its allowed list
//!   (no I/O, async, HTTP or CLI crates).
//!
//! The tag check for the allows (`// io:` / `// default:`) is the
//! concurrency census's allow-comment check, which runs over every crate.
//!
//! `--files` (the pre-commit hook's staged files): only the named `.rs`
//! files' sites and the stale entries naming them are reported on; the
//! config and dependency checks always run.

pub mod rules;

use std::collections::BTreeSet;
use std::path::Path;

use serde::Serialize;

use super::allowlist;
use super::concurrency::Tree;
use super::concurrency::rules::{boundary, is_space, is_word, matches};
use super::concurrency::scan::{Site, sites_from_hits};
use crate::HostEnv;
use crate::committed::census::{EffectSite, effects_allowlist_at};
use crate::paths::EFFECTS_ALLOWLIST;
use crate::settings::Parsed;
use rules::{
    EFFECTS, OBSERVATION_FILES, PURE_CRATES, PURE_LIB_DENY, RESULT_STRING, ROLES, STATE_TYPES,
    STATIC_STATE, is_product, pure_crate_of, rule_of,
};

const DOC: &str = "docs/responsibility.md";

/// What a census run found (before rendering).
pub struct Findings {
    pub sites: Vec<Site>,
    pub unlisted: Vec<Site>,
    pub stale: Vec<EffectSite>,
    pub entry_errors: Vec<String>,
    /// The clippy config, lib.rs and dependency checks.
    pub layer_problems: Vec<String>,
    pub live_files: usize,
    pub entries: usize,
}

impl Findings {
    fn failures(&self) -> usize {
        self.unlisted.len() + self.stale.len() + self.entry_errors.len() + self.layer_problems.len()
    }
}

// ─── matching ─────────────────────────────────────────────────────────

fn skip_ws(s: &[u8], mut i: usize) -> usize {
    while i < s.len() && is_space(s[i]) {
        i += 1;
    }
    i
}

fn word_at(s: &[u8], i: usize) -> &[u8] {
    let n = s[i.min(s.len())..]
        .iter()
        .take_while(|b| is_word(**b))
        .count();
    &s[i..i + n]
}

/// Offsets of the whole word `kw` in `s`.
fn words(s: &[u8], kw: &str) -> Vec<usize> {
    words_in(s, 0, s.len(), kw)
}

/// Offsets of the whole word `kw` lying inside `s[from..to]`.
fn words_in(s: &[u8], from: usize, to: usize, kw: &str) -> Vec<usize> {
    let k = kw.as_bytes();
    (from..to.saturating_sub(k.len().saturating_sub(1)))
        .filter(|&p| s[p..].starts_with(k) && boundary(s, p) && boundary(s, p + k.len()))
        .collect()
}

/// Whether a static's type text names interior-mutable state.
fn is_state_type(ty: &[u8]) -> bool {
    let mut i = 0;
    while i < ty.len() {
        let w = word_at(ty, i);
        if w.is_empty() {
            i += 1;
            continue;
        }
        let at_start = i == 0 || !is_word(ty[i - 1]);
        if at_start && (w.starts_with(b"Atomic") || STATE_TYPES.iter().any(|t| w == t.as_bytes())) {
            return true;
        }
        i += w.len();
    }
    false
}

/// `static mut X`, `static X: <state type>`, `thread_local!`: global
/// mutable state. A static's hit sits just after its name, so its scope
/// reads `static X` at module level.
fn static_hits(s: &[u8]) -> Vec<usize> {
    let mut out: Vec<usize> = words(s, "static")
        .into_iter()
        .filter(|&p| p == 0 || s[p - 1] != b'\'')
        .filter_map(|p| static_is_state(s, p + "static".len()))
        .collect();
    out.extend(words(s, "thread_local").into_iter().filter(|&p| {
        let e = p + "thread_local".len();
        s.get(e) == Some(&b'!')
    }));
    out
}

/// Where a state static's name ends, if the static after `after_kw` is one.
fn static_is_state(s: &[u8], after_kw: usize) -> Option<usize> {
    let i = skip_ws(s, after_kw);
    let name = word_at(s, i);
    if name == b"mut" {
        let n = skip_ws(s, i + 3);
        return Some(n + word_at(s, n).len());
    }
    if name.is_empty() {
        return None;
    }
    let name_end = i + name.len();
    let colon = skip_ws(s, name_end);
    if s.get(colon) != Some(&b':') || s.get(colon + 1) == Some(&b':') {
        return None;
    }
    let end = s[colon..]
        .iter()
        .position(|b| matches!(b, b'=' | b';'))
        .map_or(s.len(), |e| colon + e);
    is_state_type(&s[colon + 1..end]).then_some(name_end)
}

/// The end (exclusive) of a fn signature starting at `from`: the first `{`
/// or `;` outside parentheses and brackets.
fn signature_end(s: &[u8], from: usize) -> usize {
    let mut depth = 0i32;
    for (i, &b) in s.iter().enumerate().skip(from) {
        match b {
            b'(' | b'[' => depth += 1,
            b')' | b']' => depth -= 1,
            b'{' | b';' if depth <= 0 => return i,
            _ => {}
        }
    }
    s.len()
}

/// The offset of the first `->` outside parentheses in `s[from..to]`.
fn arrow(s: &[u8], from: usize, to: usize) -> Option<usize> {
    let mut depth = 0i32;
    for i in from..to.saturating_sub(1) {
        match s[i] {
            b'(' | b'[' => depth += 1,
            b')' | b']' => depth -= 1,
            b'-' if depth == 0 && s[i + 1] == b'>' => return Some(i),
            _ => {}
        }
    }
    None
}

/// The top-level generic arguments of `<...>` opening at `open`, and
/// where they end. `->` inside (a `Fn() -> T`) is not a closing bracket.
fn generic_args(s: &[u8], open: usize, to: usize) -> Option<Vec<&[u8]>> {
    let mut depth = 0i32;
    let mut args = Vec::new();
    let mut start = open + 1;
    let mut i = open;
    while i < to {
        match s[i] {
            b'-' if s.get(i + 1) == Some(&b'>') => i += 1,
            b'<' | b'(' | b'[' => depth += 1,
            b'>' | b')' | b']' => {
                depth -= 1;
                if depth == 0 {
                    args.push(&s[start..i]);
                    return Some(args);
                }
            }
            b',' if depth == 1 => {
                args.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    None
}

fn trimmed(b: &[u8]) -> &[u8] {
    let start = b.iter().position(|c| !is_space(*c)).unwrap_or(b.len());
    let end = b
        .iter()
        .rposition(|c| !is_space(*c))
        .map_or(start, |e| e + 1);
    &b[start..end]
}

/// Offsets of each `Result<_, String>` in the return type `s[from..to]`.
fn result_string_in(s: &[u8], from: usize, to: usize) -> Vec<usize> {
    words_in(s, from, to, "Result")
        .into_iter()
        .filter(|&p| {
            let open = skip_ws(s, p + "Result".len());
            s.get(open) == Some(&b'<')
                && generic_args(s, open, to)
                    .is_some_and(|a| a.len() >= 2 && trimmed(a[a.len() - 1]) == b"String")
        })
        .collect()
}

/// The text from the last `;`, `{` or `}` before `at` up to `at`.
fn header_before(s: &[u8], at: usize) -> &[u8] {
    let start = s[..at]
        .iter()
        .rposition(|b| matches!(b, b';' | b'{' | b'}'))
        .map_or(0, |p| p + 1);
    &s[start..at]
}

/// Each non-private fn (`pub`, `pub(crate)`, ...) whose return type holds
/// a `Result<_, String>`: the offset of that `Result`.
fn result_string_hits(s: &[u8]) -> Vec<usize> {
    let mut out = Vec::new();
    for p in words(s, "fn") {
        let name = skip_ws(s, p + 2);
        if word_at(s, name).is_empty() || words(header_before(s, p), "pub").is_empty() {
            continue;
        }
        let end = signature_end(s, name);
        let Some(arrow_at) = arrow(s, name, end) else {
            continue;
        };
        let ret_end = words_in(s, arrow_at, end, "where")
            .first()
            .copied()
            .unwrap_or(end);
        out.extend(result_string_in(s, arrow_at + 2, ret_end));
    }
    out
}

/// Every effect site of one sanitized product file.
pub fn sites_of(file: &str, text: &[u8]) -> Vec<Site> {
    let mut hits: Vec<(usize, &str)> = EFFECTS
        .iter()
        .flat_map(|c| matches(text, c).into_iter().map(move |at| (at, c.id)))
        .collect();
    hits.extend(static_hits(text).into_iter().map(|at| (at, STATIC_STATE)));
    hits.extend(
        result_string_hits(text)
            .into_iter()
            .map(|at| (at, RESULT_STRING)),
    );
    sites_from_hits(file, text, hits)
}

// ─── entries ──────────────────────────────────────────────────────────

/// The roles a site may carry (the first is the one suggested).
fn allowed_roles(file: &str, construct: &str) -> &'static [&'static str] {
    if construct == RESULT_STRING {
        return &["legacy"];
    }
    if pure_crate_of(file).is_none() {
        return &["shell"];
    }
    if OBSERVATION_FILES.contains(&file) {
        &["observation", "legacy"]
    } else {
        &["legacy"]
    }
}

fn entry_problems(e: &EffectSite, i: usize) -> Vec<String> {
    let (place, mut problems) = allowlist::basic_problems(
        i,
        &[
            ("file", &e.file),
            ("scope", &e.scope),
            ("construct", &e.construct),
            ("role", &e.role),
        ],
        rule_of(&e.construct).is_some(),
    );
    if !e.file.is_empty() && !is_product(&e.file) {
        problems.push(format!(
            "  {place}: not a product crate's file (the census scans {})",
            rules::PRODUCT_CRATES.join(", ")
        ));
    }
    let roles = allowed_roles(&e.file, &e.construct);
    if !e.role.is_empty() && !roles.contains(&e.role.as_str()) {
        let meaning: Vec<String> = ROLES
            .iter()
            .filter(|(r, _)| roles.contains(r))
            .map(|(r, why)| format!("{r} = {why}"))
            .collect();
        problems.push(format!(
            "  {place}: role \"{}\" is not allowed here — one of: {}",
            e.role,
            meaning.join("; ")
        ));
    }
    problems
}

#[derive(Serialize)]
struct Suggested<'a> {
    file: &'a str,
    scope: &'a str,
    construct: &'a str,
    role: &'a str,
}

fn unlisted_message(s: &Site) -> String {
    let role = allowed_roles(&s.file, &s.construct)[0];
    let entry = Suggested {
        file: &s.file,
        scope: &s.scope,
        construct: &s.construct,
        role,
    };
    let json = serde_json::to_string_pretty(&entry).unwrap_or_default();
    let lines: Vec<String> = s.lines.iter().map(ToString::to_string).collect();
    let advice = match (pure_crate_of(&s.file), s.construct.as_str()) {
        (_, RESULT_STRING) => {
            "Return a typed error (an enum with Display) instead; the shell formats it once."
        }
        (Some(_), _) => {
            "This is a pure crate: move the effect to the shell (cli) and hand core the value, or inject a trait. Listing it as legacy grows a list that only shrinks."
        }
        (None, _) => "A shell crate: confirm the site and add it to the inventory.",
    };
    [
        format!(
            "  {}:{}  {}  —  {}",
            s.file,
            lines.join(","),
            s.scope,
            s.construct
        ),
        format!("    Rule: {}", rule_of(&s.construct).unwrap_or("")),
        format!("    {advice}"),
        format!("    To confirm it, add to {EFFECTS_ALLOWLIST}:"),
        json.lines()
            .map(|l| format!("      {l}"))
            .collect::<Vec<_>>()
            .join("\n"),
    ]
    .join("\n")
}

// ─── the static layer: clippy config, lib.rs, dependencies ───────────

fn read_toml(path: &Path) -> Result<toml::Table, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    text.parse::<toml::Table>()
        .map_err(|e| format!("{}: {e}", path.display()))
}

/// Every root entry missing from `cfg`, named under `label`.
fn missing_root_entries(root_cfg: &toml::Table, cfg: &toml::Table, label: &str) -> Vec<String> {
    let mut out = Vec::new();
    for (k, v) in root_cfg {
        match (v, cfg.get(k)) {
            (toml::Value::Array(want), Some(toml::Value::Array(have))) => {
                out.extend(want.iter().filter(|e| !have.contains(e)).map(|e| {
                    format!("  {label}: `{k}` lacks the root entry {e} (clippy does not merge configs: the root ban does not apply to this crate)")
                }));
            }
            (want, Some(have)) if want == have => {}
            (want, have) => out.push(format!(
                "  {label}: `{k}` is {} where the root clippy.toml has {want}",
                have.map_or("absent".to_string(), ToString::to_string)
            )),
        }
    }
    out
}

/// The pure crates' clippy configs: each holds the root's entries, and all
/// agree with the first.
fn clippy_problems(root: &Path, pure: &[&str]) -> Vec<String> {
    let root_cfg = match read_toml(&root.join("clippy.toml")) {
        Ok(t) => t,
        Err(e) => return vec![format!("  {e}")],
    };
    let mut out = Vec::new();
    let mut first: Option<(String, toml::Table)> = None;
    for c in pure {
        let label = format!("{c}clippy.toml");
        let cfg = match read_toml(&root.join(&label)) {
            Ok(t) => t,
            Err(e) => {
                out.push(format!(
                    "  {e} (a pure crate needs its own clippy.toml with the effect bans)"
                ));
                continue;
            }
        };
        out.extend(missing_root_entries(&root_cfg, &cfg, &label));
        match &first {
            Some((first_label, first_cfg)) if *first_cfg != cfg => out.push(format!(
                "  {label} differs from {first_label}: the pure crates share one set of bans"
            )),
            Some(_) => {}
            None => first = Some((label, cfg)),
        }
    }
    out
}

/// Each pure crate's lib.rs keeps the swallowed-error deny line.
fn lib_problems(root: &Path, pure: &[&str]) -> Vec<String> {
    pure.iter()
        .filter_map(|c| {
            let lib = root.join(c).join("src/lib.rs");
            match std::fs::read_to_string(&lib) {
                Ok(t) if t.lines().any(|l| l.trim() == PURE_LIB_DENY) => None,
                Ok(_) => Some(format!("  {c}src/lib.rs lacks `{PURE_LIB_DENY}`")),
                Err(e) => Some(format!("  {}: {e}", lib.display())),
            }
        })
        .collect()
}

/// Each pure crate's normal dependencies are within its allowed list; no
/// build or target-specific dependencies.
pub fn dependency_problems(root: &Path, pure: &[(&str, &[&str])]) -> Vec<String> {
    let mut out = Vec::new();
    for (c, allowed) in pure {
        let manifest = match read_toml(&root.join(c).join("Cargo.toml")) {
            Ok(t) => t,
            Err(e) => {
                out.push(format!("  {e}"));
                continue;
            }
        };
        let deps = manifest.get("dependencies").and_then(toml::Value::as_table);
        for name in deps.into_iter().flat_map(|d| d.keys()) {
            if !allowed.contains(&name.as_str()) {
                out.push(format!(
                    "  {c}Cargo.toml: dependency `{name}` is not on the pure crate's allowed list ({}); an I/O, async or CLI crate belongs in the shell (rules::PURE_CRATES)",
                    allowed.join(", ")
                ));
            }
        }
        for table in ["build-dependencies", "target"] {
            if manifest.contains_key(table) {
                out.push(format!("  {c}Cargo.toml: a pure crate has no [{table}]"));
            }
        }
    }
    out
}

/// The static-layer checks over the pure crates present under `root`.
fn layer_problems(root: &Path) -> Vec<String> {
    let present: Vec<(&str, &[&str])> = PURE_CRATES
        .iter()
        .copied()
        .filter(|(c, _)| root.join(c).join("Cargo.toml").exists())
        .collect();
    let names: Vec<&str> = present.iter().map(|p| p.0).collect();
    let mut out = clippy_problems(root, &names);
    out.extend(lib_problems(root, &names));
    out.extend(dependency_problems(root, &present));
    out
}

// ─── the run ──────────────────────────────────────────────────────────

/// Run the census over `root` against `entries`; `files` narrows the site
/// report to those root-relative paths (None: everything).
pub fn census(
    root: &Path,
    entries: &[EffectSite],
    files: Option<&BTreeSet<String>>,
) -> Result<Findings, String> {
    let tree = Tree::scan(root)?;
    let live: Vec<_> = tree
        .live()
        .into_iter()
        .filter(|s| is_product(&s.rel))
        .collect();
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
    Ok(Findings {
        live_files: live.len(),
        entries: entries.len(),
        layer_problems: layer_problems(root),
        sites,
        unlisted: checked.unlisted,
        stale: checked.stale,
        entry_errors: checked.entry_errors,
    })
}

/// Entry counts by role, for the summary line.
fn role_counts(entries: usize, sites: &[Site]) -> String {
    let legacy_pure = sites
        .iter()
        .filter(|s| pure_crate_of(&s.file).is_some() && s.construct != RESULT_STRING)
        .filter(|s| !OBSERVATION_FILES.contains(&s.file.as_str()))
        .count();
    let strings = sites
        .iter()
        .filter(|s| s.construct == RESULT_STRING)
        .count();
    format!(
        "{} sites ({legacy_pure} effect sites in the pure crates outside profiling, {strings} Result<_, String> signatures), {entries} allowlist entries",
        sites.len()
    )
}

/// The report text and its exit code.
fn render(f: &Findings, named: Option<usize>, ms: u128) -> (u8, String) {
    let mut out = String::new();
    allowlist::section(
        &mut out,
        format!(
            "EFFECTS CENSUS: {} unlisted site(s) — the pure crates do no I/O ({DOC})",
            f.unlisted.len()
        ),
        &f.unlisted.iter().map(unlisted_message).collect::<Vec<_>>(),
    );
    allowlist::section(
        &mut out,
        format!(
            "EFFECTS CENSUS: {} stale allowlist entr{} — the site is gone (or moved to another function); delete or re-key it in {EFFECTS_ALLOWLIST}: the list only shrinks",
            f.stale.len(),
            if f.stale.len() == 1 { "y" } else { "ies" }
        ),
        &f.stale
            .iter()
            .map(|e| format!("  {}  {}  —  {}", e.file, e.scope, e.construct))
            .collect::<Vec<_>>(),
    );
    allowlist::section(
        &mut out,
        "EFFECTS CENSUS: malformed allowlist entries".to_string(),
        &f.entry_errors,
    );
    allowlist::section(
        &mut out,
        "EFFECTS CENSUS: the pure crates' clippy config, lib.rs lints or dependencies".to_string(),
        &f.layer_problems,
    );
    let scope = named.map_or("all files".to_string(), |n| format!("{n} named file(s)"));
    let summary = format!(
        "{} in {} non-test product files, {scope} checked, {ms} ms",
        role_counts(f.entries, &f.sites),
        f.live_files
    );
    if f.failures() > 0 {
        out.push_str(&format!("\nEFFECTS CENSUS RED — {summary}\n"));
        (1, out)
    } else {
        out.push_str(&format!("EFFECTS CENSUS GREEN — {summary}\n"));
        (0, out)
    }
}

/// Run the verb.
pub fn run(p: &Parsed, host: &HostEnv) -> (u8, String, String) {
    allowlist::run_verb(
        (p, host),
        ("effects", EFFECTS_ALLOWLIST),
        |f| effects_allowlist_at(f).map(|a| a.value.entries),
        |root, entries: &Vec<EffectSite>, files| census(root, entries, files),
        render,
    )
}

#[cfg(test)]
#[path = "effects_test.rs"]
mod effects_test;
