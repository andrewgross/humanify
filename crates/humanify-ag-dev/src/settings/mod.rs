//! The harness's ONE parse point (docs/dev-harness.md §2.2): a verb's
//! argv → its positionals and its settings, resolved against
//! [`HARNESS`]. Everything is refused BEFORE anything runs, and every
//! refusal is collected: a retired spelling (naming its replacement), an
//! unknown flag (with a "did you mean"), a flag that is not a setting of
//! this verb, a wrong type or range, and a `--pipeline-set` entry the
//! PIPELINE's registry refuses (unknown key, retired key, wrong type or
//! range, or a key the harness sets itself).
//!
//! A run's settings are written once, as one run config file
//! ([`run_config`], the JS spelling); its sha goes into the run identity.

pub mod keys;

use std::sync::LazyLock;

use clap::{Arg, ArgAction};
use humanify_ag_cli::config::keys::PIPELINE;
use humanify_ag_cli::registry::flags::{FlagSpec, add_flags, flag_values, specs};
use humanify_ag_cli::registry::{Registry, Resolved, Val, check_range, resolve};
use humanify_ag_model::js::{JsObject, JsValue};

pub use keys::{HARNESS, HARNESS_KEYS};

static FLAGS: LazyLock<Vec<FlagSpec>> = LazyLock::new(|| specs(&HARNESS));

/// A verb's parsed command line.
#[derive(Debug)]
pub struct Parsed {
    pub positionals: Vec<String>,
    pub settings: Resolved,
    /// The `--pipeline-set` entries, checked and typed, in order.
    pub pipeline_sets: Vec<(String, Val)>,
}

/// Whether `dotted` is one of `accepted` (a key, or a whole section).
fn accepts(accepted: &[&str], dotted: &str) -> bool {
    accepted
        .iter()
        .any(|a| dotted == *a || dotted.starts_with(&format!("{a}.")))
}

/// Every flag spelling the harness accepts (`--x`, a boolean's `--no-x`).
fn spellings() -> Vec<String> {
    FLAGS
        .iter()
        .flat_map(|f| {
            std::iter::once(format!("--{}", f.long))
                .chain(f.negated.iter().map(|n| format!("--{n}")))
        })
        .collect()
}

fn unknown_flags(args: &[String]) -> Vec<String> {
    let known = spellings();
    args.iter()
        .take_while(|a| *a != "--")
        .filter(|a| a.starts_with('-') && a.len() > 1)
        .filter_map(|a| {
            let name = a.split_once('=').map_or(a.as_str(), |(n, _)| n);
            if known.iter().any(|k| k == name) {
                return None;
            }
            Some(match HARNESS.nearest_flag(name) {
                Some(near) => format!("unknown flag {name} (did you mean {near}?)"),
                None => format!("unknown flag {name}"),
            })
        })
        .collect()
}

/// Parse `verb`'s arguments. `accepted` lists the keys or sections the
/// verb reads; `harness_owned` the PIPELINE keys the verb writes itself
/// (refused in `--pipeline-set`).
pub fn parse(
    verb: &str,
    accepted: &[&str],
    harness_owned: &[&str],
    args: &[String],
) -> Result<Parsed, Vec<String>> {
    if let Some(refusal) = HARNESS.retired_flag(args) {
        return Err(vec![refusal.trim_end().to_string()]);
    }
    let unknown = unknown_flags(args);
    if !unknown.is_empty() {
        return Err(unknown);
    }
    let cmd = add_flags(
        clap::Command::default()
            .name("humanify-ag-dev")
            .no_binary_name(true)
            .disable_help_flag(true)
            .arg(Arg::new("positional").action(ArgAction::Append).num_args(1)),
        FLAGS.as_slice(),
    );
    let m = cmd
        .try_get_matches_from(args)
        .map_err(|e| vec![e.render().to_string().trim_end().to_string()])?;
    let given = flag_values(FLAGS.as_slice(), &m).map_err(|e| vec![e.trim_end().to_string()])?;
    let mut errors: Vec<String> = given
        .iter()
        .filter(|(dotted, _)| !accepts(accepted, dotted))
        .map(|(dotted, v)| {
            format!(
                "{} is not a setting of `{verb}` (it sets {dotted})",
                HARNESS.key(dotted).flag_for(v)
            )
        })
        .collect();
    let (settings, resolve_errors) = resolve(&HARNESS, &given, None);
    errors.extend(resolve_errors);
    let sets = settings.list("run.pipeline_set").unwrap_or_default();
    let pipeline_sets = match check_pipeline_sets(&sets, harness_owned) {
        Ok(s) => s,
        Err(e) => {
            errors.extend(e);
            Vec::new()
        }
    };
    if !errors.is_empty() {
        return Err(errors);
    }
    let positionals = m
        .get_many::<String>("positional")
        .map(|v| v.cloned().collect())
        .unwrap_or_default();
    Ok(Parsed {
        positionals,
        settings,
        pipeline_sets,
    })
}

/// Check `--pipeline-set section.key=value` entries against the PIPELINE
/// registry linked into this binary (HEAD's; an older binary's keys are
/// checked at its first launch, docs/dev-harness.md §2.2): the key exists
/// and is not retired, the harness does not set it itself, the value is
/// typed and in range. Every refusal is collected.
pub fn check_pipeline_sets(
    entries: &[String],
    harness_owned: &[&str],
) -> Result<Vec<(String, Val)>, Vec<String>> {
    let mut out = Vec::new();
    let mut errors = Vec::new();
    for entry in entries {
        match check_one(&PIPELINE, entry, harness_owned) {
            Ok(v) => out.push(v),
            Err(e) => errors.push(format!("--pipeline-set {entry}: {e}")),
        }
    }
    if errors.is_empty() {
        Ok(out)
    } else {
        Err(errors)
    }
}

fn check_one(
    registry: &Registry,
    entry: &str,
    harness_owned: &[&str],
) -> Result<(String, Val), String> {
    let (dotted, raw) = entry
        .split_once('=')
        .ok_or_else(|| "not section.key=value".to_string())?;
    if let Some((_, note)) = registry.retired_keys.iter().find(|(k, _)| *k == dotted) {
        return Err(format!("{dotted} is retired ({note})"));
    }
    let key = registry
        .find(dotted)
        .ok_or_else(|| registry.unknown_key("the pipeline's registry", dotted))?;
    if harness_owned.contains(&dotted) {
        return Err(format!(
            "{dotted} is set by the harness itself for this verb; use its own flag"
        ));
    }
    let val = Registry::typed_text(key, dotted, raw)?;
    if let Some(e) = check_range(key, dotted, &val) {
        return Err(e);
    }
    Ok((dotted.to_string(), val))
}

/// HEAD's compiled-in default of a PIPELINE key (the linked registry —
/// never a hand copy like eval.ts's `BINARY_DEFAULT_MATCHING`).
pub fn pipeline_default(dotted: &str) -> Option<Val> {
    (PIPELINE.key(dotted).default)()
}

fn js_of(v: &Val) -> JsValue {
    match v {
        Val::Text(s) => JsValue::str(s.as_str()),
        Val::Bool(b) => JsValue::Bool(*b),
        Val::Int(n) => JsValue::Number(*n as f64),
        Val::Float(x) => JsValue::Number(*x),
        Val::List(items) => JsValue::str_array(items),
    }
}

/// The run config a verb writes ONCE: the verb, its positionals, every
/// setting it reads (`accepted`) with its value — unset keys as `null` —
/// and the checked `--pipeline-set` entries in order.
pub fn run_config(verb: &str, accepted: &[&str], parsed: &Parsed) -> JsValue {
    let mut settings = JsObject::new();
    for key in HARNESS.keys {
        let dotted = key.dotted();
        if !accepts(accepted, &dotted) {
            continue;
        }
        let value = parsed
            .settings
            .values()
            .find(|(k, _, _)| *k == dotted)
            .map_or(JsValue::Null, |(_, v, _)| js_of(v));
        settings.insert(dotted, value);
    }
    let mut out = JsObject::new();
    out.insert("verb", JsValue::str(verb));
    out.insert("positionals", JsValue::str_array(&parsed.positionals));
    out.insert("settings", JsValue::Object(settings));
    out.insert(
        "pipelineSettings",
        JsValue::Array(
            parsed
                .pipeline_sets
                .iter()
                .map(|(k, v)| {
                    let mut o = JsObject::new();
                    o.insert("key", JsValue::str(k.as_str()));
                    o.insert("value", js_of(v));
                    JsValue::Object(o)
                })
                .collect(),
        ),
    );
    JsValue::Object(out)
}

#[cfg(test)]
#[path = "settings_test.rs"]
mod settings_test;
