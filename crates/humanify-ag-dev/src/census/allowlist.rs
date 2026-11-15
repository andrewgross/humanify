//! What the allowlist censuses (concurrency, effects) share: the verb's
//! run (inputs, load, census, render), an entry's common problems, and
//! matching the found sites against the entries (unlisted, stale,
//! duplicate).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::Instant;

use super::concurrency::scan::Site;
use super::concurrency::{named_files, normalize};
use crate::HostEnv;
use crate::paths::repo_root;
use crate::settings::Parsed;

/// The root, allowlist file and `--files` narrowing a census runs with.
struct Inputs {
    root: PathBuf,
    allowlist: PathBuf,
    files: Option<BTreeSet<String>>,
}

/// `--root` (default: this repository), `--allowlist` (default:
/// `default_allowlist` in this repository) and `--files`, resolved against
/// the working directory.
fn inputs(p: &Parsed, host: &HostEnv, default_allowlist: &str) -> Inputs {
    let cwd = host.cwd.clone().unwrap_or_else(repo_root);
    let abs = |s: String| normalize(&cwd.join(s));
    let root = p
        .settings
        .text("dev.root")
        .map_or_else(|| normalize(&repo_root()), abs);
    let allowlist = p
        .settings
        .text("dev.allowlist")
        .map_or_else(|| normalize(&repo_root().join(default_allowlist)), abs);
    let files = named_files(
        &p.settings.list("dev.files").unwrap_or_default(),
        &root,
        &cwd,
    );
    Inputs {
        root,
        allowlist,
        files,
    }
}

fn or_q(s: &str) -> &str {
    if s.is_empty() { "?" } else { s }
}

/// `entry {i} (file scope construct)`, with `?` for an empty field.
fn place(i: usize, file: &str, scope: &str, construct: &str) -> String {
    format!(
        "entry {i} ({} {} {})",
        or_q(file),
        or_q(scope),
        or_q(construct)
    )
}

/// An entry's name and its common problems: one line per empty field
/// (`fields[..3]` are file, scope, construct) and an unknown construct.
pub fn basic_problems(
    i: usize,
    fields: &[(&str, &String)],
    known_construct: bool,
) -> (String, Vec<String>) {
    let place = place(i, fields[0].1, fields[1].1, fields[2].1);
    let mut problems: Vec<String> = fields
        .iter()
        .filter(|(_, v)| v.trim().is_empty())
        .map(|(k, _)| format!("  {place}: missing \"{k}\""))
        .collect();
    if !fields[2].1.is_empty() && !known_construct {
        problems.push(format!("  {place}: unknown construct \"{}\"", fields[2].1));
    }
    (place, problems)
}

/// `file|scope|construct`.
fn key(file: &str, scope: &str, construct: &str) -> String {
    format!("{file}|{scope}|{construct}")
}

/// A census's verdict on its entries.
pub struct Checked<E> {
    pub unlisted: Vec<Site>,
    pub stale: Vec<E>,
    /// Each entry's problems, then the duplicates.
    pub entry_errors: Vec<String>,
}

/// Check `entries` against `sites`: `key_of` names an entry's (file,
/// scope, construct), `problems` its malformed fields.
pub fn check<E: Clone>(
    sites: &[Site],
    entries: &[E],
    key_of: fn(&E) -> (&str, &str, &str),
    problems: fn(&E, usize) -> Vec<String>,
    files: Option<&BTreeSet<String>>,
) -> Checked<E> {
    let keys: Vec<(&str, &str, &str)> = entries.iter().map(key_of).collect();
    let matched = match_entries(sites, &keys, files);
    let mut entry_errors: Vec<String> = entries
        .iter()
        .enumerate()
        .flat_map(|(i, e)| problems(e, i))
        .collect();
    entry_errors.extend(matched.duplicates);
    Checked {
        unlisted: matched.unlisted,
        stale: matched.stale.iter().map(|&i| entries[i].clone()).collect(),
        entry_errors,
    }
}

/// Append a titled section when it has lines.
pub fn section(out: &mut String, title: String, lines: &[String]) {
    if !lines.is_empty() {
        out.push_str(&format!("\n{title}\n{}\n", lines.join("\n\n")));
    }
}

/// A census over (root, its loaded allowlist, the `--files` narrowing).
pub type CensusFn<A, F> = fn(&Path, &A, Option<&BTreeSet<String>>) -> Result<F, String>;

/// A census verb from end to end: resolve the inputs, load the allowlist,
/// run the census, render it with the number of named files and the time.
pub fn run_verb<A, F>(
    (p, host): (&Parsed, &HostEnv),
    (name, default_allowlist): (&str, &str),
    load: fn(&Path) -> Result<A, String>,
    census: CensusFn<A, F>,
    render: fn(&F, Option<usize>, u128) -> (u8, String),
) -> (u8, String, String) {
    let started = Instant::now();
    let inputs = inputs(p, host, default_allowlist);
    let files = inputs.files.as_ref();
    match load(&inputs.allowlist).and_then(|a| census(&inputs.root, &a, files)) {
        Ok(f) => {
            let (code, out) = render(&f, files.map(BTreeSet::len), started.elapsed().as_millis());
            (code, out, String::new())
        }
        Err(e) => (1, String::new(), format!("census {name}: {e}\n")),
    }
}

/// Sites against entries keyed (file, scope, construct).
struct Matched {
    /// Sites no entry lists (in scope).
    unlisted: Vec<Site>,
    /// Indices of entries whose site is gone (in scope).
    stale: Vec<usize>,
    /// `duplicate entry: <key>` lines.
    duplicates: Vec<String>,
}

/// Match `sites` against `entries` (file, scope, construct); `files`
/// narrows unlisted and stale to those files (None: everything).
fn match_entries(
    sites: &[Site],
    entries: &[(&str, &str, &str)],
    files: Option<&BTreeSet<String>>,
) -> Matched {
    let in_scope = |rel: &str| files.is_none_or(|f| f.contains(rel));
    let mut seen = BTreeSet::new();
    let mut duplicates = Vec::new();
    for (file, scope, construct) in entries {
        let k = key(file, scope, construct);
        if !seen.insert(k.clone()) {
            duplicates.push(format!("  duplicate entry: {k}"));
        }
    }
    let site_keys: BTreeSet<String> = sites
        .iter()
        .map(|s| key(&s.file, &s.scope, &s.construct))
        .collect();
    let unlisted = sites
        .iter()
        .filter(|s| in_scope(&s.file) && !seen.contains(&key(&s.file, &s.scope, &s.construct)))
        .cloned()
        .collect();
    let stale = entries
        .iter()
        .enumerate()
        .filter(|(_, (file, scope, construct))| {
            in_scope(file) && !site_keys.contains(&key(file, scope, construct))
        })
        .map(|(i, _)| i)
        .collect();
    Matched {
        unlisted,
        stale,
        duplicates,
    }
}
