//! The generic settings-registry machinery, shared by BOTH binaries
//! (harness lane L1, docs/dev-harness.md §2.2): the pipeline
//! (`humanify-ag`, its table is `crate::config::keys::PIPELINE`) and the
//! measurement harness (`humanify-ag-dev`, its `HARNESS` table).
//!
//! A [`Registry`] is ONE table that declares every setting once: section,
//! name, type and range ([`Kind`]), compiled-in default, meaning, and the
//! flag it generates ([`Key::flag_name`]). Every key is a flag and every
//! flag is a key; the clap command is built from the table
//! (`registry::flags`). This module owns what does not depend on WHICH
//! table it reads: typed values, range checks, "did you mean", resolving
//! flag / file / default with the conflict rule, the rendered example, and
//! the refusal of retired or renamed spellings. Extracted 2026-11-07 from
//! `config/file.rs` and `surface.rs` without changing a byte of the
//! pipeline's help, example file or resolution (the goldens and
//! config_file_test.rs hold that).

use std::collections::BTreeMap;

pub mod flags;

#[cfg(test)]
mod registry_test;

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
    pub fn describe(&self) -> &'static str {
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
    /// The flag's spelling when it is not the derived one
    /// (`--<section>-<name>`, [`Key::flag_name`]): the short names of the
    /// commonest settings (`--endpoint`, `--split`, `--bundler`).
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

    /// The key's flag (exp179): its override, else `--<section>-<name>`
    /// with dots and underscores as dashes, the section's last word
    /// dropped when the name already starts with it (`naming.naming_floor`
    /// is `--naming-floor`, `llm.concurrency` is `--llm-concurrency`). A
    /// boolean also has `--no-<flag>`.
    pub fn flag_name(&self) -> String {
        if let Some(f) = self.flag {
            return f.to_string();
        }
        let mut words: Vec<&str> = self.section.split('.').collect();
        if words
            .last()
            .is_some_and(|last| self.name.starts_with(&format!("{last}_")))
        {
            words.pop();
        }
        words.push(self.name);
        format!("--{}", words.join("-").replace('_', "-"))
    }

    /// The flag as written for `v`: a boolean's false is its `--no-` form.
    pub fn flag_for(&self, v: &Val) -> String {
        match v {
            Val::Bool(false) => format!("--no-{}", self.flag_name().trim_start_matches("--")),
            _ => self.flag_name(),
        }
    }
}

/// One binary's settings table and the rules its command line follows.
pub struct Registry {
    /// Every setting, in the order help and the example list them.
    pub keys: &'static [Key],
    /// Keys renamed or never valid: `(dotted key, what to do instead)`.
    /// Refused by name, never aliased.
    pub retired_keys: &'static [(&'static str, &'static str)],
    /// Flags renamed or retired: `(old spelling, the refusal's note)`.
    /// Refused by name before the parser runs, never aliased.
    pub retired_flags: &'static [(&'static str, &'static str)],
    /// Single-letter aliases: `(dotted key, letter)`.
    pub short_flags: &'static [(&'static str, char)],
    /// The one counted flag (`-vv`), if any.
    pub counted_flag: Option<&'static str>,
    /// List keys whose flag is REPEATED, one item per use, order kept
    /// (`--pipeline-set a=1 --pipeline-set b=2`), instead of one
    /// comma-separated value.
    pub repeatable: &'static [&'static str],
    /// Each section's help heading.
    pub section_titles: &'static [(&'static str, &'static str)],
    /// Sections the short help (`-h`) leaves out.
    pub short_help_hides: fn(&str) -> bool,
    /// A value flag's placeholder when the kind's default is not it:
    /// `(dotted key, placeholder)`.
    pub value_names: &'static [(&'static str, &'static str)],
}

/// A value a flag gave: (dotted key, value).
pub type FlagValue = (&'static str, Val);

impl Registry {
    /// The key `dotted` names. A typo is a programming error.
    pub fn key(&self, dotted: &str) -> &'static Key {
        self.find(dotted)
            .unwrap_or_else(|| panic!("no setting {dotted} in the registry"))
    }

    pub fn find(&self, dotted: &str) -> Option<&'static Key> {
        self.keys.iter().find(|k| k.dotted() == dotted)
    }

    /// Whether `dotted` is a section (or a parent of one).
    pub fn is_section(&self, dotted: &str) -> bool {
        self.keys
            .iter()
            .any(|k| k.section == dotted || k.section.starts_with(&format!("{dotted}.")))
    }

    /// "unknown key X (did you mean Y?)", the nearest key within edit
    /// distance 3.
    pub fn unknown_key(&self, label: &str, dotted: &str) -> String {
        let near = self
            .keys
            .iter()
            .map(|k| k.dotted())
            .min_by_key(|k| distance(k, dotted))
            .filter(|k| distance(k, dotted) <= 3);
        match near {
            Some(k) => format!("{label}: unknown key {dotted} (did you mean {k}?)"),
            None => format!("{label}: unknown key {dotted}"),
        }
    }

    /// The nearest accepted flag spelling to `flag` (`--x` form), within
    /// edit distance 3 — for an unknown flag's "did you mean".
    pub fn nearest_flag(&self, flag: &str) -> Option<String> {
        self.keys
            .iter()
            .flat_map(|k| {
                let f = k.flag_name();
                let no = matches!(k.kind, Kind::Bool)
                    .then(|| format!("--no-{}", f.trim_start_matches("--")));
                std::iter::once(f).chain(no)
            })
            .min_by_key(|f| distance(f, flag))
            .filter(|f| distance(f, flag) <= 3)
    }

    /// The refusal of a retired or renamed flag in `args`, if any.
    /// `--flag=value` counts too; nothing after `--` is a flag.
    pub fn retired_flag(&self, args: &[String]) -> Option<String> {
        retired_in(self.retired_flags, args)
    }

    /// The typed value of the key's text (a flag's argument, or the value
    /// of a `section.key=value` setting); the range is checked by
    /// [`check_range`]. `who` names it as written.
    pub fn typed_text(key: &Key, who: &str, raw: &str) -> Result<Val, String> {
        let bad = |what: &str| format!("invalid value '{raw}' for '{who}': {what}");
        match key.kind {
            Kind::Int { .. } => raw
                .parse::<u64>()
                .map(Val::Int)
                .map_err(|_| bad("a non-negative integer is expected")),
            Kind::Float { .. } => raw
                .parse::<f64>()
                .map(Val::Float)
                .map_err(|_| bad("a number is expected")),
            Kind::Bool => match raw {
                "true" => Ok(Val::Bool(true)),
                "false" => Ok(Val::Bool(false)),
                _ => Err(bad("true or false is expected")),
            },
            Kind::List(_) => Ok(Val::List(
                raw.split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect(),
            )),
            _ => Ok(Val::Text(raw.to_string())),
        }
    }
}

/// The refusal of the first `args` entry named in `table`, if any.
pub fn retired_in(table: &[(&str, &str)], args: &[String]) -> Option<String> {
    args.iter().take_while(|a| *a != "--").find_map(|arg| {
        let name = arg.split_once('=').map_or(arg.as_str(), |(n, _)| n);
        table
            .iter()
            .find(|(flag, _)| *flag == name)
            .map(|(flag, note)| format!("error: unexpected argument '{flag}' ({note})\n"))
    })
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
#[derive(Clone)]
pub struct Resolved {
    registry: &'static Registry,
    values: BTreeMap<String, (Val, Source)>,
}

impl std::fmt::Debug for Resolved {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Resolved")
            .field("values", &self.values)
            .finish()
    }
}

impl Resolved {
    fn get(&self, dotted: &str) -> Option<&Val> {
        // A typo in a getter is a programming error, caught by every test.
        let _ = self.registry.key(dotted);
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

    /// Every resolved value, dotted, in key order: what a run config
    /// records.
    pub fn values(&self) -> impl Iterator<Item = (&str, &Val, Source)> {
        self.values.iter().map(|(k, (v, s))| (k.as_str(), v, *s))
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
pub fn distance(a: &str, b: &str) -> usize {
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

/// The values a settings FILE gave, already checked key by key: where it
/// came from, what it set acceptably, what it set that was refused.
pub struct FileValues<'a> {
    pub path: &'a str,
    pub values: &'a BTreeMap<String, Val>,
    pub problems: &'a [String],
}

/// Resolve every key: the flag, else the file, else the default. A flag
/// and the file that DISAGREE are refused, naming both sources and values;
/// a flag value is range-checked under the flag's own name. The refusals
/// come back beside the (then unusable) resolution, so the caller can keep
/// collecting.
pub fn resolve(
    registry: &'static Registry,
    flags: &[FlagValue],
    file: Option<FileValues<'_>>,
) -> (Resolved, Vec<String>) {
    let mut errors: Vec<String> = file
        .as_ref()
        .map(|f| f.problems.to_vec())
        .unwrap_or_default();
    let mut values = BTreeMap::new();
    for key in registry.keys {
        let dotted = key.dotted();
        let flag = flags.iter().find(|(k, _)| *k == dotted).map(|(_, v)| v);
        let from_file = file.as_ref().and_then(|f| f.values.get(&dotted));
        let flag_name = flag.map(|v| key.flag_for(v)).unwrap_or_default();
        // A refused value leaves the key at its default, so the checks
        // between keys still read every key and report their own refusals
        // beside this one (the resolution is unusable either way).
        let refused = || (key.default)().map(|d| (d, Source::Default));
        let entry = match (flag, from_file) {
            (Some(a), _) if check_range(key, &flag_name, a).is_some() => {
                errors.extend(check_range(key, &flag_name, a));
                refused()
            }
            (Some(a), Some(b)) if a != b => {
                errors.push(format!(
                    "config conflict: {dotted} = {} ({flag_name}) vs {} ({})",
                    a.render(),
                    b.render(),
                    file.as_ref().map_or("", |f| f.path)
                ));
                refused()
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
    (Resolved { registry, values }, errors)
}

/// The example settings file: `header`, then every key, its flag, its
/// default and its meaning, rendered from the registry. Every key line is
/// commented out (exp179): every key is also a flag, and a line copied
/// verbatim would make its flag a conflict.
pub fn render_example(registry: &Registry, header: &str) -> String {
    let mut out = String::from(header);
    let mut section = "";
    for key in registry.keys {
        if key.section != section {
            section = key.section;
            out.push_str(&format!("\n[{section}]\n"));
        }
        out.push_str(&format!("# {}\n# flag: {}\n", key.doc, key.flag_name()));
        let line = match (key.default)() {
            Some(v) => format!("# {} = {}", key.name, v.render()),
            None => format!("# {} =   (unset: {})", key.name, key.unset),
        };
        out.push_str(&line);
        out.push('\n');
    }
    out
}
