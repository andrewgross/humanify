//! What the e2e stage and ask-trace hand the pipeline binary, and how a
//! launch is logged: the `--config` TOML writer (successor of
//! `experiments/lib/pipeline-config.ts` for these two launchers), bash's
//! `printf %q` (scripts/lib/launch-golden.ts `shellQuote`), and the launch
//! log the goldens under `experiments/034-eval-harness/launch-goldens/`
//! hold, line for line in the recorder shape (test/launch-recorder.ts).
//!
//! Private to `e2e` on purpose: lane L3 owns `src/launch/` and writes its
//! runners' config files there; when both have landed, the two writers
//! fold into one (reported to the L4 lead).

use std::path::Path;

use humanify_ag_model::js::{JsValue, stringify};

/// One `--config` setting: a dotted key and its value (a string, a
/// number, a boolean or a list of strings).
pub type Setting = (String, JsValue);

/// A setting from a dotted key and a value.
pub fn setting(key: &str, value: JsValue) -> Setting {
    (key.to_string(), value)
}

/// The TOML text: one table per section, in first-seen order; a key set
/// twice is refused (two layers of the harness disagreeing about one
/// setting is the conflict the binary refuses between a flag and the file).
pub fn render_config(settings: &[Setting]) -> Result<String, String> {
    let mut seen: Vec<&str> = Vec::new();
    let mut sections: Vec<(&str, Vec<String>)> = Vec::new();
    for (key, value) in settings {
        if let Some(first) = settings.iter().find(|(k, _)| k == key)
            && seen.contains(&key.as_str())
        {
            return Err(format!(
                "{key} is set twice ({} and {})",
                stringify(&first.1),
                stringify(value)
            ));
        }
        seen.push(key);
        let (section, name) = key
            .rsplit_once('.')
            .ok_or_else(|| format!("\"{key}\" is not a dotted config key (section.key)"))?;
        // JSON strings, numbers and booleans are valid TOML literals; a
        // list is spelled `["a", "b"]`.
        let rendered = match value {
            JsValue::Array(items) => format!(
                "[{}]",
                items.iter().map(stringify).collect::<Vec<_>>().join(", ")
            ),
            other => stringify(other),
        };
        let line = format!("{name} = {rendered}");
        match sections.iter_mut().find(|(s, _)| *s == section) {
            Some((_, lines)) => lines.push(line),
            None => sections.push((section, vec![line])),
        }
    }
    Ok(sections
        .iter()
        .map(|(s, lines)| format!("[{s}]\n{}\n", lines.join("\n")))
        .collect())
}

/// Write `settings` as the TOML file `file`.
pub fn write_config(file: &Path, settings: &[Setting]) -> Result<(), String> {
    let text = render_config(settings)?;
    std::fs::write(file, text).map_err(|e| format!("{}: {e}", file.display()))
}

/// One argument as bash 5.2's `printf %q` writes it: the recorder shells
/// log argv that way, so a log written here matches theirs byte for byte.
/// `~` and `#` are quoted only first; non-ASCII passes through. A control
/// character is refused (no launch here passes one).
pub fn shell_quote(arg: &str) -> Result<String, String> {
    if arg.is_empty() {
        return Ok("''".into());
    }
    let mut out = String::new();
    for (i, ch) in arg.chars().enumerate() {
        let code = ch as u32;
        if code < 0x20 || code == 0x7f {
            return Err(format!("shell_quote: control character in {arg:?}"));
        }
        let safe = ch.is_ascii_alphanumeric()
            || "_./:=+@%-".contains(ch)
            || code > 0x7f
            || (i > 0 && (ch == '~' || ch == '#'));
        if !safe {
            out.push('\\');
        }
        out.push(ch);
    }
    Ok(out)
}

/// Every launch a stage makes, in order, as the goldens store them.
#[derive(Default)]
pub struct LaunchLog {
    entries: Vec<String>,
}

impl LaunchLog {
    /// Record one launch: `head` is what precedes the API key (the binary,
    /// and for ask-trace its working directory), then argv through
    /// `printf %q`, then the `--config` file's lines, prefixed, one
    /// trailing space trimmed.
    pub fn record(&mut self, head: &str, api_key: &str, argv: &[String]) -> Result<(), String> {
        let mut quoted = String::new();
        for a in argv {
            quoted.push_str(&shell_quote(a)?);
            quoted.push(' ');
        }
        let mut lines = vec![format!("{head} HUMANIFY_API_KEY={api_key} :: {quoted}")];
        let config = argv
            .iter()
            .position(|a| a == "--config")
            .and_then(|at| argv.get(at + 1));
        if let Some(config) = config
            && let Ok(text) = std::fs::read_to_string(config)
        {
            let text = text.strip_suffix('\n').unwrap_or(&text);
            for line in text.split('\n') {
                let l = format!("humanify-config| {line}");
                lines.push(l.strip_suffix(' ').unwrap_or(&l).to_string());
            }
        }
        self.entries.push(format!("{}\n", lines.join("\n")));
        Ok(())
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The log with each `(from, to)` replaced, in order.
    pub fn normalized(&self, replacements: &[(String, String)]) -> String {
        let mut text = self.entries.concat();
        for (from, to) in replacements {
            text = text.replace(from.as_str(), to);
        }
        text
    }
}

/// The first line where two logs differ, for a failure message.
pub fn first_difference(actual: &str, expected: &str) -> String {
    let a: Vec<&str> = actual.split('\n').collect();
    let e: Vec<&str> = expected.split('\n').collect();
    for i in 0..a.len().max(e.len()) {
        if a.get(i) != e.get(i) {
            return format!(
                "line {}:\n  golden: {}\n  now:    {}",
                i + 1,
                e.get(i).copied().unwrap_or("<end>"),
                a.get(i).copied().unwrap_or("<end>")
            );
        }
    }
    "no line differs".into()
}

/// `http://127.0.0.1:<digits>/v1` → `http://127.0.0.1:<PORT>/v1`, every
/// occurrence.
pub fn normalize_port(text: &str) -> String {
    const HEAD: &str = "http://127.0.0.1:";
    let mut out = String::new();
    let mut rest = text;
    while let Some(i) = rest.find(HEAD) {
        out.push_str(&rest[..i + HEAD.len()]);
        let after = &rest[i + HEAD.len()..];
        let digits = after.chars().take_while(char::is_ascii_digit).count();
        if digits > 0 && after[digits..].starts_with("/v1") {
            out.push_str("<PORT>");
            rest = &after[digits..];
        } else {
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
#[path = "launch_log_test.rs"]
mod launch_log_test;
