//! Every launch line the harness makes, built in ONE place
//! (docs/dev-harness.md §2.1 `src/launch/`): the pipeline's `--config`
//! TOML file (the port of `experiments/lib/pipeline-config.ts`, THE writer
//! of a launch's settings) and the pipeline binary's operational argv.
//!
//! The launch goldens recorded in exp191 / run-launch.test.ts are the
//! contract: every runner's recorder log must equal its golden byte for
//! byte, so the TOML rendering and the argv order here are the TS
//! launchers' exactly.
//!
//! A setting is given as `section.key=value`, in the order the launcher
//! lists them. The value is read as JSON when it parses as a number, a
//! boolean, a string or an array of strings (`472`, `true`, `["a"]`), and
//! as a plain string otherwise (`low`, `/a/path`). A key set twice is
//! refused: two layers of the harness disagreeing about one setting is
//! the conflict the binary refuses between a flag and its file.

use humanify_ag_model::js::{JsValue, stringify};

/// One `section.key=value` setting, typed the way pipeline-config.ts
/// types it.
#[derive(Clone, Debug, PartialEq)]
pub struct Setting {
    pub key: String,
    /// The TOML literal (JSON text is valid TOML for these kinds).
    pub literal: String,
}

fn is_dotted_key(key: &str) -> bool {
    let parts: Vec<&str> = key.split('.').collect();
    parts.len() >= 2
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_lowercase() || c == '_'))
}

/// `section.key=value` → a [`Setting`]; Err on a malformed entry.
pub fn parse_setting(entry: &str) -> Result<Setting, String> {
    let (key, raw) = entry
        .split_once('=')
        .ok_or_else(|| format!("\"{entry}\" is not section.key=value"))?;
    if !is_dotted_key(key) {
        return Err(format!(
            "\"{key}\" is not a dotted config key (section.key)"
        ));
    }
    let literal = match JsValue::parse(raw) {
        Ok(v @ (JsValue::Number(_) | JsValue::Bool(_) | JsValue::String(_))) => stringify(&v),
        Ok(JsValue::Array(items)) if items.iter().all(|x| matches!(x, JsValue::String(_))) => {
            let parts: Vec<String> = items.iter().map(stringify).collect();
            format!("[{}]", parts.join(", "))
        }
        _ => stringify(&JsValue::str(raw)),
    };
    Ok(Setting {
        key: key.to_string(),
        literal,
    })
}

/// The TOML text of `entries`: one table per section, sections in
/// first-seen order, keys in the order given. Err on a malformed entry or
/// a key set twice.
pub fn render_config(entries: &[String]) -> Result<String, String> {
    let mut seen: Vec<Setting> = Vec::new();
    for entry in entries {
        let s = parse_setting(entry)?;
        if let Some(prev) = seen.iter().find(|p| p.key == s.key) {
            return Err(format!(
                "{} is set twice ({} and {})",
                s.key, prev.literal, s.literal
            ));
        }
        seen.push(s);
    }
    let mut sections: Vec<(String, Vec<String>)> = Vec::new();
    for s in &seen {
        let (section, name) = s.key.rsplit_once('.').expect("a dotted key");
        let line = format!("{name} = {}", s.literal);
        match sections.iter_mut().find(|(n, _)| n == section) {
            Some((_, lines)) => lines.push(line),
            None => sections.push((section.to_string(), vec![line])),
        }
    }
    Ok(sections
        .iter()
        .map(|(s, lines)| format!("[{s}]\n{}\n", lines.join("\n")))
        .collect())
}

/// Write one launch's config file; returns its text.
pub fn write_config(path: &std::path::Path, entries: &[String]) -> Result<String, String> {
    let text = render_config(entries).map_err(|e| format!("pipeline-config: {e}"))?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    std::fs::write(path, &text).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(text)
}

/// The pipeline binary's operational argv for one humanify launch, in the
/// order every golden holds: input, `--config`, `--endpoint`, `--model`,
/// `-o`, `--prior-version` (when there is a prior), then `-vv` (when
/// verbose).
#[derive(Clone, Debug)]
pub struct HumanifyArgs<'a> {
    pub input: &'a str,
    pub config: &'a str,
    pub endpoint: &'a str,
    pub model: &'a str,
    pub out: &'a str,
    pub prior: Option<&'a str>,
    pub verbose: bool,
}

impl HumanifyArgs<'_> {
    pub fn argv(&self) -> Vec<String> {
        let mut a: Vec<String> = [
            self.input,
            "--config",
            self.config,
            "--endpoint",
            self.endpoint,
            "--model",
            self.model,
            "-o",
            self.out,
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        if let Some(p) = self.prior {
            a.push("--prior-version".into());
            a.push(p.to_string());
        }
        if self.verbose {
            a.push("-vv".into());
        }
        a
    }
}

/// The settings every eval launch carries first (run.sh `pipeline_config`,
/// neutrality.sh `leg_flags`, walk.sh): the model's effort and
/// concurrency, and the split.
pub fn base_settings(effort: &str, concurrency: u32) -> Vec<String> {
    vec![
        format!("llm.reasoning_effort={effort}"),
        format!("llm.concurrency={concurrency}"),
        "split.enabled=true".to_string(),
    ]
}

#[cfg(test)]
#[path = "launch_test.rs"]
mod launch_test;
