//! A registry's flags for clap (exp179's one-to-one rule, generic since
//! harness lane L1): one flag per key, named by [`Key::flag_name`], taking
//! the value the key's kind implies (a boolean is `--x` / `--no-x`, a list
//! is comma-separated or — for a [`Registry::repeatable`] key — repeated,
//! the counted flag is `-vv`), with the key's doc as its help, grouped by
//! section. The pipeline (`crate::surface`) and the harness
//! (`humanify-ag-dev`'s settings) build their commands from this.

use clap::{Arg, ArgAction, ArgMatches};

use super::{FlagValue, Key, Kind, Registry, Val};

/// One key's flag, as the command line spells it.
pub struct FlagSpec {
    pub key: &'static Key,
    /// The key, dotted (`llm.concurrency`).
    pub dotted: String,
    /// The long name without its dashes (`llm-concurrency`).
    pub long: String,
    /// A boolean's off switch (`no-split`).
    pub negated: Option<String>,
    pub short: Option<char>,
    /// Its `--help` heading (`LLM [llm]`).
    pub heading: String,
    /// A value flag's placeholder (`n`, `off|report|names`).
    pub value_name: String,
    /// Repeated, one list item per use.
    pub repeated: bool,
    /// The registry's counted flag (`-vv`).
    pub counted: bool,
    /// Left out of the short help.
    pub hide_short: bool,
}

impl FlagSpec {
    pub fn of(registry: &Registry, key: &'static Key) -> FlagSpec {
        let long = key.flag_name().trim_start_matches("--").to_string();
        let dotted = key.dotted();
        FlagSpec {
            negated: matches!(key.kind, Kind::Bool).then(|| format!("no-{long}")),
            short: registry
                .short_flags
                .iter()
                .find(|(d, _)| *d == dotted)
                .map(|(_, c)| *c),
            heading: heading(registry, key.section),
            value_name: value_name(registry, key),
            repeated: registry.repeatable.contains(&dotted.as_str()),
            counted: registry.counted_flag == Some(dotted.as_str()),
            hide_short: (registry.short_help_hides)(key.section),
            long,
            dotted,
            key,
        }
    }
}

/// Every key's flag, in registry order.
pub fn specs(registry: &'static Registry) -> Vec<FlagSpec> {
    registry
        .keys
        .iter()
        .map(|k| FlagSpec::of(registry, k))
        .collect()
}

/// What a value flag's placeholder says (`<n>`, `<off|report|names>`).
fn value_name(registry: &Registry, key: &Key) -> String {
    let dotted = key.dotted();
    if let Some((_, name)) = registry.value_names.iter().find(|(d, _)| *d == dotted) {
        return name.to_string();
    }
    match (&key.kind, dotted.as_str()) {
        (Kind::Url, _) => "url".to_string(),
        (Kind::Int { .. }, _) => "n".to_string(),
        (Kind::Float { .. }, _) => "x".to_string(),
        (Kind::List(_), d) if registry.repeatable.contains(&d) => "value".to_string(),
        (Kind::List(_), _) => "a,b,...".to_string(),
        (Kind::Choice(names), _) => {
            let joined = names().join("|");
            if joined.len() <= 40 {
                joined
            } else {
                "name".to_string()
            }
        }
        (_, d) if d.ends_with("_dir") || d.ends_with("index_store") => "dir".to_string(),
        _ => "path".to_string(),
    }
}

/// The doc's first sentence, at most ~100 characters: the `-h` line.
fn summary(doc: &str) -> String {
    let first = doc.find(". ").map_or(doc, |i| &doc[..=i]);
    if first.chars().count() <= 100 {
        return first.trim_end_matches('.').to_string();
    }
    let cut: String = first.chars().take(97).collect();
    let cut = cut.rsplit_once(' ').map_or(cut.as_str(), |(head, _)| head);
    format!("{cut}...")
}

/// What `--help` adds after the doc: the default and the config key.
fn footnote(spec: &FlagSpec) -> String {
    let key = spec.key;
    let default = match ((key.default)(), &spec.negated) {
        (Some(Val::Bool(b)), Some(no)) => {
            format!(
                "default: {}; --{no} turns it off",
                if b { "on" } else { "off" }
            )
        }
        (Some(v), _) => format!("default: {}", v.render()),
        (None, _) => format!("unset: {}", key.unset),
    };
    format!("[{default}] [config: {}]", key.dotted())
}

fn heading(registry: &Registry, section: &str) -> String {
    let title = registry
        .section_titles
        .iter()
        .find(|(s, _)| *s == section)
        .map_or(section, |(_, t)| *t);
    format!("{title} [{section}]")
}

fn flag_arg(spec: &'static FlagSpec) -> Arg {
    let key = spec.key;
    let mut arg = Arg::new(spec.long.as_str())
        .long(spec.long.as_str())
        .help(summary(key.doc))
        .long_help(format!("{}\n{}", key.doc, footnote(spec)))
        .help_heading(spec.heading.as_str())
        .hide_short_help(spec.hide_short);
    if let Some(c) = spec.short {
        arg = arg.short(c);
    }
    match key.kind {
        Kind::Bool => arg.action(ArgAction::SetTrue).conflicts_with(
            spec.negated
                .as_deref()
                .expect("a boolean has an off switch"),
        ),
        _ if spec.counted => arg.action(ArgAction::Count),
        _ if spec.repeated => arg
            .action(ArgAction::Append)
            .value_name(spec.value_name.as_str()),
        _ => arg
            .action(ArgAction::Set)
            .value_name(spec.value_name.as_str()),
    }
}

/// A boolean's `--no-x`: listed by its `--x` line, not on its own.
fn negated_arg(spec: &'static FlagSpec) -> Option<Arg> {
    let no = spec.negated.as_deref()?;
    Some(Arg::new(no).long(no).action(ArgAction::SetTrue).hide(true))
}

/// Add every spec's flag (and a boolean's `--no-` form) to `cmd`.
pub fn add_flags(mut cmd: clap::Command, specs: &'static [FlagSpec]) -> clap::Command {
    for spec in specs {
        cmd = cmd.arg(flag_arg(spec));
        if let Some(no) = negated_arg(spec) {
            cmd = cmd.arg(no);
        }
    }
    cmd
}

/// A flag's text as the key's value: typed by the key's kind (a range is
/// checked later, by the resolver, under the flag's name).
fn typed_flag(spec: &FlagSpec, raw: &str) -> Result<Val, String> {
    let bad = |what: &str| {
        format!(
            "error: invalid value '{raw}' for '--{} <{}>': {what}\n",
            spec.long, spec.value_name
        )
    };
    match spec.key.kind {
        Kind::Int { .. } => raw
            .parse::<u64>()
            .map(Val::Int)
            .map_err(|_| bad("a non-negative integer is expected")),
        Kind::Float { .. } => raw
            .parse::<f64>()
            .map(Val::Float)
            .map_err(|_| bad("a number is expected")),
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

/// Every flag given, as (key, value), in registry order.
pub fn flag_values(specs: &'static [FlagSpec], m: &ArgMatches) -> Result<Vec<FlagValue>, String> {
    let mut out = Vec::new();
    for spec in specs {
        let dotted: &'static str = spec.dotted.as_str();
        let value = match &spec.negated {
            Some(no) if m.get_flag(no) => Some(Val::Bool(false)),
            Some(_) => m.get_flag(&spec.long).then_some(Val::Bool(true)),
            None if spec.counted => {
                let n = m.get_count(&spec.long);
                (n > 0).then(|| Val::Int(u64::from(n)))
            }
            None if spec.repeated => m
                .get_many::<String>(&spec.long)
                .map(|items| Val::List(items.cloned().collect())),
            None => match m.get_one::<String>(&spec.long) {
                Some(raw) => Some(typed_flag(spec, raw)?),
                None => None,
            },
        };
        if let Some(v) = value {
            out.push((dotted, v));
        }
    }
    Ok(out)
}

/// A combined single-dash argument other than the counted flag's (`-vv`):
/// clap would read `-c5` as `-c 5` and `-vc 5` as `-v -c 5`; the harness
/// writes neither, so both are refused rather than guessed at.
pub fn attached_short(args: &[String]) -> Option<String> {
    args.iter()
        .take_while(|a| *a != "--")
        .find(|a| {
            a.len() > 2
                && a.starts_with('-')
                && !a.starts_with("--")
                && !a[1..].bytes().all(|b| b == b'v')
        })
        .map(|a| {
            format!(
                "error: unexpected argument '{a}': a short option's value is a separate \
                 argument (`-o out`, not `-oout`), and only -v repeats (`-vv`)\n"
            )
        })
}
