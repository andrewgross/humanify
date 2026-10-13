//! The `--config <file>` TOML file (config sweep step 5, 2026-10-13;
//! docs/config-consolidation.md §3).
//!
//! Every setting is a KEY in the [`KEYS`] registry: its section and name,
//! its type and range, its compiled-in default, its one-line meaning, and —
//! for the few operational settings that are also flags — the flag. The
//! registry is the ONE place a setting is declared: the file is checked
//! against it (strict keys, typed values, ranges), the flag/file conflict
//! check reads it, and `humanify.example.toml` is rendered from it
//! ([`render_example`]; a test holds the committed file byte-equal).
//!
//! Refusals are loud and collected, before any work (§3.1): a file that is
//! missing or not TOML, an unknown key (with a "did you mean"), a retired
//! key (naming its replacement), a wrong type, an out-of-range value, and a
//! flag that disagrees with the file. A flag and the file agreeing is fine,
//! and the record says both set it.
//!
//! The file is parsed into a plain `toml::Table` and walked here; nothing
//! else in the workspace reads TOML (tests/env_guard.rs).

use std::collections::BTreeMap;

use sha2::{Digest, Sha256};

/// A setting's value, as the file or a flag gave it.
#[derive(Clone, Debug, PartialEq)]
pub enum Val {
    Text(String),
    Bool(bool),
    Int(u64),
    Float(f64),
    List(Vec<String>),
}

impl Val {
    /// The value as TOML spells it (strings quoted, lists inline) — for
    /// messages and the rendered example.
    pub fn render(&self) -> String {
        let quote = |s: &str| serde_json::to_string(s).expect("a string serializes");
        match self {
            Val::Text(s) => quote(s),
            Val::Bool(b) => b.to_string(),
            Val::Int(n) => n.to_string(),
            Val::Float(x) => {
                let s = x.to_string();
                if s.contains('.') { s } else { format!("{s}.0") }
            }
            Val::List(items) => {
                let inner: Vec<String> = items.iter().map(|s| quote(s)).collect();
                format!("[{}]", inner.join(", "))
            }
        }
    }
}

/// What a key accepts.
pub enum Kind {
    /// A non-empty string (a path, a model name).
    Text,
    /// An `http://` or `https://` URL.
    Url,
    Bool,
    /// An integer in `min..=max`.
    Int {
        min: u64,
        max: u64,
    },
    /// A number in `min..=max` (an integer is accepted).
    Float {
        min: f64,
        max: f64,
    },
    /// One of a fixed set of names.
    Choice(fn() -> Vec<String>),
    /// A list of names: from a fixed set, or (None) checked by the
    /// key's owner (the switch registry lists its own valid set).
    List(Option<fn() -> Vec<String>>),
}

impl Kind {
    fn describe(&self) -> &'static str {
        match self {
            Kind::Text | Kind::Url | Kind::Choice(_) => "a string",
            Kind::Bool => "a boolean (true or false)",
            Kind::Int { .. } => "an integer",
            Kind::Float { .. } => "a number",
            Kind::List(_) => "a list of strings",
        }
    }
}

/// One setting.
pub struct Key {
    /// The TOML table (`naming`, `library.lookup`).
    pub section: &'static str,
    pub name: &'static str,
    pub kind: Kind,
    /// The operational flag that also sets it (`--endpoint`), if any.
    pub flag: Option<&'static str>,
    /// The compiled-in default; None = unset (`unset` says what that means).
    pub default: fn() -> Option<Val>,
    /// What "unset" means, for a key with no default value.
    pub unset: &'static str,
    /// One line: what the setting does.
    pub doc: &'static str,
}

impl Key {
    pub fn dotted(&self) -> String {
        format!("{}.{}", self.section, self.name)
    }
}

/// Keys that were renamed or never belong in the file: `(dotted key, what
/// to do instead)`. Refused by name, never aliased.
pub const RETIRED_KEYS: &[(&str, &str)] = &[
    (
        "llm.api_key",
        "a secret never goes in the config file (it lands in launch records): set \
         HUMANIFY_API_KEY in the environment",
    ),
    ("perf.profile", "renamed: perf.perf_trace"),
    (
        "naming.sequential",
        "moved: naming.schedule = \"sequential\"",
    ),
    (
        "io.input",
        "the input is the positional argument, not a setting",
    ),
];

/// The parsed, checked config file: where it came from and what it set.
#[derive(Clone, Debug)]
pub struct ConfigFile {
    /// The path as given (`--config <path>`).
    pub path: String,
    /// sha256 of the file's bytes.
    pub sha256: String,
    /// Every key it set ACCEPTABLY, dotted.
    pub values: BTreeMap<String, Val>,
    /// Every key it set that was refused (unknown, retired, wrong type,
    /// out of range), as messages — reported by `Config::build` together
    /// with every other refusal, so the user sees all of them at once.
    pub problems: Vec<String>,
}

/// Where a resolved value came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Default,
    Flag,
    File,
    /// The flag and the file agreed.
    Both,
}

impl Source {
    pub fn name(self) -> &'static str {
        match self {
            Source::Default => "default",
            Source::Flag => "flag",
            Source::File => "file",
            Source::Both => "flag+file",
        }
    }
}

/// The run's settings, each resolved from flag, file or default.
#[derive(Clone, Debug, Default)]
pub struct Resolved {
    values: BTreeMap<String, (Val, Source)>,
}

fn key_of(dotted: &str) -> &'static Key {
    super::keys::KEYS
        .iter()
        .find(|k| k.dotted() == dotted)
        .unwrap_or_else(|| panic!("no setting {dotted} in the registry"))
}

impl Resolved {
    fn get(&self, dotted: &str) -> Option<&Val> {
        // A typo in a getter is a programming error, caught by every test.
        let _ = key_of(dotted);
        self.values.get(dotted).map(|(v, _)| v)
    }

    pub fn text(&self, dotted: &str) -> Option<String> {
        match self.get(dotted)? {
            Val::Text(s) => Some(s.clone()),
            other => panic!("{dotted} is not a string: {other:?}"),
        }
    }

    pub fn flag_on(&self, dotted: &str) -> bool {
        match self.get(dotted) {
            Some(Val::Bool(b)) => *b,
            other => panic!("{dotted} is not a boolean: {other:?}"),
        }
    }

    pub fn int(&self, dotted: &str) -> Option<u64> {
        match self.get(dotted)? {
            Val::Int(n) => Some(*n),
            other => panic!("{dotted} is not an integer: {other:?}"),
        }
    }

    pub fn float(&self, dotted: &str) -> f64 {
        match self.get(dotted) {
            Some(Val::Float(x)) => *x,
            Some(Val::Int(n)) => *n as f64,
            other => panic!("{dotted} is not a number: {other:?}"),
        }
    }

    pub fn list(&self, dotted: &str) -> Option<Vec<String>> {
        match self.get(dotted)? {
            Val::List(items) => Some(items.clone()),
            other => panic!("{dotted} is not a list: {other:?}"),
        }
    }

    pub fn source(&self, dotted: &str) -> Source {
        self.values
            .get(dotted)
            .map_or(Source::Default, |(_, source)| *source)
    }

    /// Every value that is not the compiled-in default's: key → source.
    pub fn sources(&self) -> BTreeMap<String, &'static str> {
        self.values
            .iter()
            .filter(|(_, (_, s))| *s != Source::Default)
            .map(|(k, (_, s))| (k.clone(), s.name()))
            .collect()
    }
}

/// `Val` from a TOML value, or a message naming the key and what it got.
fn typed(key: &Key, v: &toml::Value) -> Result<Val, String> {
    let got = || format!("{} must be {}, got {v}", key.dotted(), key.kind.describe());
    match (&key.kind, v) {
        (Kind::Text | Kind::Url | Kind::Choice(_), toml::Value::String(s)) => {
            Ok(Val::Text(s.clone()))
        }
        (Kind::Bool, toml::Value::Boolean(b)) => Ok(Val::Bool(*b)),
        (Kind::Int { .. }, toml::Value::Integer(n)) => u64::try_from(*n)
            .map(Val::Int)
            .map_err(|_| format!("{} = {n} is negative", key.dotted())),
        (Kind::Float { .. }, toml::Value::Float(x)) => Ok(Val::Float(*x)),
        (Kind::Float { .. }, toml::Value::Integer(n)) => Ok(Val::Float(*n as f64)),
        (Kind::List(_), toml::Value::Array(items)) => items
            .iter()
            .map(|i| i.as_str().map(str::to_string).ok_or_else(got))
            .collect::<Result<Vec<_>, _>>()
            .map(Val::List),
        _ => Err(got()),
    }
}

fn outside_names(dotted: &str, got: &str, allowed: &[String]) -> String {
    format!(
        "{dotted} must be one of: {} (got \"{got}\")",
        allowed.join(", ")
    )
}

/// Range and form: what a typed value must also satisfy. `who` names the
/// setting as the user wrote it (`--endpoint`, or the dotted key).
pub fn check_range(key: &Key, who: &str, v: &Val) -> Option<String> {
    match (&key.kind, v) {
        (Kind::Text | Kind::Url | Kind::Choice(_), Val::Text(s)) if s.trim().is_empty() => {
            Some(format!("{who} must not be empty"))
        }
        (Kind::Url, Val::Text(s)) if !(s.starts_with("http://") || s.starts_with("https://")) => {
            Some(format!(
                "{who} must be an http:// or https:// URL (got \"{s}\")"
            ))
        }
        (Kind::Choice(names), Val::Text(s)) => {
            let allowed = names();
            (!allowed.contains(s)).then(|| outside_names(who, s, &allowed))
        }
        (Kind::Int { min, max }, Val::Int(n)) if n < min || n > max => {
            Some(int_range(who, *n, *min, *max))
        }
        (Kind::Float { min, max }, Val::Float(x)) if x < min || x > max || x.is_nan() => {
            Some(format!("{who} = {x} is outside {min}..{max}"))
        }
        (Kind::List(Some(names)), Val::List(items)) => {
            let allowed = names();
            items
                .iter()
                .find(|i| !allowed.contains(i))
                .map(|bad| outside_names(who, bad, &allowed))
        }
        _ => None,
    }
}

fn int_range(who: &str, n: u64, min: u64, max: u64) -> String {
    if n < min {
        format!("{who} must be at least {min} (got {n})")
    } else {
        format!("{who} = {n} is outside {min}..={max}")
    }
}

/// Plain edit distance, for "did you mean".
fn distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut prev = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cur = row[j + 1];
            row[j + 1] = (prev + usize::from(ca != *cb)).min(row[j] + 1).min(cur + 1);
            prev = cur;
        }
    }
    row[b.len()]
}

fn unknown_key(label: &str, dotted: &str) -> String {
    let near = super::keys::KEYS
        .iter()
        .map(|k| k.dotted())
        .min_by_key(|k| distance(k, dotted))
        .filter(|k| distance(k, dotted) <= 3);
    match near {
        Some(k) => format!("{label}: unknown key {dotted} (did you mean {k}?)"),
        None => format!("{label}: unknown key {dotted}"),
    }
}

fn is_section(dotted: &str) -> bool {
    super::keys::KEYS
        .iter()
        .any(|k| k.section == dotted || k.section.starts_with(&format!("{dotted}.")))
}

/// Walk one table: every entry is a known section, a known key, or a
/// refusal.
fn walk(
    label: &str,
    prefix: &str,
    table: &toml::Table,
    values: &mut BTreeMap<String, Val>,
    errors: &mut Vec<String>,
) {
    for (name, v) in table {
        let dotted = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{prefix}.{name}")
        };
        if let Some((_, note)) = RETIRED_KEYS.iter().find(|(k, _)| *k == dotted) {
            errors.push(format!("{label}: {dotted} is retired ({note})"));
            continue;
        }
        if let (toml::Value::Table(inner), true) = (v, is_section(&dotted)) {
            walk(label, &dotted, inner, values, errors);
            continue;
        }
        let Some(key) = super::keys::KEYS.iter().find(|k| k.dotted() == dotted) else {
            errors.push(unknown_key(label, &dotted));
            continue;
        };
        match typed(key, v) {
            Ok(val) => match check_range(key, &dotted, &val) {
                Some(e) => errors.push(format!("{label}: {e}")),
                None => {
                    values.insert(dotted, val);
                }
            },
            Err(e) => errors.push(format!("{label}: {e}")),
        }
    }
}

impl ConfigFile {
    /// Read `path`. Refused here: a file that cannot be read or is not
    /// TOML; every refused KEY is kept in `problems`.
    pub fn read(path: &str) -> Result<ConfigFile, Vec<String>> {
        let bytes = std::fs::read(path)
            .map_err(|e| vec![format!("--config {path}: cannot read the config file: {e}")])?;
        let text = String::from_utf8(bytes)
            .map_err(|_| vec![format!("--config {path}: the config file is not UTF-8")])?;
        ConfigFile::parse(path, &text)
    }

    /// Parse `text` (the file named `path`): Err only when it is not TOML;
    /// a refused key lands in `problems`.
    pub fn parse(path: &str, text: &str) -> Result<ConfigFile, Vec<String>> {
        let table: toml::Table = text.parse().map_err(|e: toml::de::Error| {
            vec![format!("{path}: not valid TOML: {}", e.message())]
        })?;
        let mut values = BTreeMap::new();
        let mut problems = Vec::new();
        walk(path, "", &table, &mut values, &mut problems);
        Ok(ConfigFile {
            path: path.to_string(),
            sha256: format!("{:x}", Sha256::digest(text.as_bytes())),
            values,
            problems,
        })
    }

    /// `parse`, refusing any problem: the file alone, fully checked.
    pub fn check(path: &str, text: &str) -> Result<ConfigFile, Vec<String>> {
        let f = ConfigFile::parse(path, text)?;
        if f.problems.is_empty() {
            Ok(f)
        } else {
            Err(f.problems)
        }
    }
}

/// A value an operational flag gave: (dotted key, value).
pub type FlagValue = (&'static str, Val);

/// Resolve every key: the flag, else the file, else the default. A flag
/// and the file that DISAGREE are refused, naming both sources and values;
/// a flag value is range-checked under the flag's own name. The refusals
/// come back beside the (then unusable) resolution, so the caller can keep
/// collecting.
pub fn resolve(flags: &[FlagValue], file: Option<&ConfigFile>) -> (Resolved, Vec<String>) {
    let mut errors: Vec<String> = file.map(|f| f.problems.clone()).unwrap_or_default();
    let mut values = BTreeMap::new();
    for key in super::keys::KEYS {
        let dotted = key.dotted();
        let flag = flags.iter().find(|(k, _)| *k == dotted).map(|(_, v)| v);
        let from_file = file.and_then(|f| f.values.get(&dotted));
        let flag_name = key.flag.unwrap_or("a flag");
        if let Some(e) = flag.and_then(|v| check_range(key, flag_name, v)) {
            errors.push(e);
            continue;
        }
        let entry = match (flag, from_file) {
            (Some(a), Some(b)) if a != b => {
                errors.push(format!(
                    "config conflict: {dotted} = {} ({flag_name}) vs {} ({})",
                    a.render(),
                    b.render(),
                    file.map_or("", |f| f.path.as_str())
                ));
                continue;
            }
            (Some(a), Some(_)) => Some((a.clone(), Source::Both)),
            (Some(a), None) => Some((a.clone(), Source::Flag)),
            (None, Some(b)) => Some((b.clone(), Source::File)),
            (None, None) => (key.default)().map(|d| (d, Source::Default)),
        };
        if let Some(e) = entry {
            values.insert(dotted, e);
        }
    }
    (Resolved { values }, errors)
}

/// `humanify.example.toml`: every key, its default and its meaning,
/// rendered from the registry. A key with no default is commented out.
pub fn render_example() -> String {
    let mut out = String::from(HEADER);
    let mut section = "";
    for key in super::keys::KEYS {
        if key.section != section {
            section = key.section;
            out.push_str(&format!("\n[{section}]\n"));
        }
        out.push_str(&format!("# {}\n", key.doc));
        // A key that is also a flag is commented out: copied verbatim, it
        // would make its flag a conflict.
        let line = match ((key.default)(), key.flag) {
            (Some(v), None) => format!("{} = {}", key.name, v.render()),
            (Some(v), Some(f)) => {
                format!("# {} = {}   (usually the flag {f})", key.name, v.render())
            }
            (None, Some(f)) => format!(
                "# {} =   (unset: {}; usually the flag {f})",
                key.name, key.unset
            ),
            (None, None) => format!("# {} =   (unset: {})", key.name, key.unset),
        };
        out.push_str(&line);
        out.push('\n');
    }
    out
}

const HEADER: &str = "\
# humanify.example.toml — every setting `humanify --config <file>` reads,
# at its compiled-in default. GENERATED from the key registry
# (crates/humanify-cli/src/config/keys.rs); a test holds this file
# byte-equal to the registry, so it cannot drift. Regenerate with
#   cargo test -p humanify-cli write_the_example_config -- --ignored
#
# A key left out keeps its default. Unknown keys, wrong types, out-of-range
# values and retired keys are refused before any work, naming the key. The
# operational flags (input, -o, --prior-version, --endpoint, --model,
# --config, -v) may also be given on the command line; a flag and this
# file that DISAGREE are refused, naming both. The API key is never a
# setting: it is read from HUMANIFY_API_KEY only.
";
