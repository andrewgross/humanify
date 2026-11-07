//! The `--config <file>` TOML file (config sweep step 5, 2026-10-13;
//! docs/config-consolidation.md §3).
//!
//! Every setting is a KEY in the [`KEYS`] registry: its section and name,
//! its type and range, its compiled-in default, its one-line meaning, and
//! its flag — every key is a flag, every flag a key (exp179; the name is
//! derived, [`Key::flag_name`]). The registry is the ONE place a setting is
//! declared: the file is checked against it (strict keys, typed values,
//! ranges), the command line is generated from it (`crate::surface`), the
//! flag/file conflict check reads it, and `humanify.example.toml` is
//! rendered from it ([`render_example`]; a test holds the committed file
//! byte-equal).
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

use crate::registry::{FileValues, Registry};
pub use crate::registry::{FlagValue, Key, Kind, Resolved, Source, Val, check_range};

/// The pipeline's registry: every walk and lookup here reads it.
fn registry() -> &'static Registry {
    &super::keys::PIPELINE
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
        if let (toml::Value::Table(inner), true) = (v, registry().is_section(&dotted)) {
            walk(label, &dotted, inner, values, errors);
            continue;
        }
        let Some(key) = registry().find(&dotted) else {
            errors.push(registry().unknown_key(label, &dotted));
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

/// Resolve every pipeline key: the flag, else the file, else the default
/// (`crate::registry::resolve`, the conflict rule and its messages).
pub fn resolve(flags: &[FlagValue], file: Option<&ConfigFile>) -> (Resolved, Vec<String>) {
    let file = file.map(|f| FileValues {
        path: f.path.as_str(),
        values: &f.values,
        problems: &f.problems,
    });
    crate::registry::resolve(registry(), flags, file)
}

/// `humanify.example.toml`: every key, its flag, its default and its
/// meaning, rendered from the registry. Every key line is commented out
/// (exp179): every key is also a flag, and a line copied verbatim would
/// make its flag a conflict — uncomment what the file should set.
pub fn render_example() -> String {
    crate::registry::render_example(registry(), HEADER)
}

const HEADER: &str = "\
# humanify.example.toml — every setting `humanify-ag --config <file>` reads,
# at its compiled-in default. GENERATED from the key registry
# (crates/humanify-ag-cli/src/config/keys.rs); a test holds this file
# byte-equal to the registry, so it cannot drift. Regenerate with
#   cargo test -p humanify-ag-cli write_the_example_config -- --ignored
#
# Every line below is commented out at its default: uncomment what this
# file should set. A key left out keeps its default. Unknown keys, wrong
# types, out-of-range values and retired keys are refused before any work,
# naming the key. Every key is also a command-line flag (shown above it;
# a boolean's `--no-` form turns it off); a flag and this file that
# DISAGREE are refused, naming both. The API key is never a setting: it is
# read from HUMANIFY_API_KEY in the environment.
";
