//! The configuration guard: ONE owner reads configuration (config sweep
//! step 4, docs/config-consolidation.md §3.5). It walks every crate's
//! non-test sources and fails:
//!
//! 1. a `std::env` read (or `dotenvy`) outside `humanify-cli/src/env.rs`,
//!    the one process-input reader;
//! 2. a call into that reader (`env::get`, `env::user_args`) outside the
//!    parse point — `config.rs` (the API key) and `main.rs` (argv);
//! 3. an argument parser (`clap`) outside the parse point — `surface.rs`
//!    (the pipeline's declaration), `main.rs` (the stage verbs'),
//!    `config.rs` — and the index builder, a separate binary;
//! 4. a config-file reader (`toml`) outside `config.rs`;
//! 5. a JSON lookup of a setting name (`v["skipLibraries"]`,
//!    `.get("model")`, `.bool(…)`) outside `config.rs`: a setting is read
//!    from the typed `Config`, never re-read from a JSON map.
//!
//! Clippy's `disallowed_methods` ban on `std::env::var` is the static layer
//! under rule 1. A source file's test code — `*_test.rs`, `tests/`, and an
//! inline `#[cfg(test)] mod … { … }` (the file's tail, by convention) — is
//! not guarded; code after a one-line `#[cfg(test)] mod x_test;` is.

use std::path::Path;

/// One rule: what it forbids, and the files allowed to do it.
struct Rule {
    name: &'static str,
    /// The line matches the forbidden construct.
    hit: fn(&str) -> bool,
    /// Paths (suffixes) exempt from the rule, each with its reason.
    owners: &'static [(&'static str, &'static str)],
}

const ENV_MODULE: (&str, &str) = ("humanify-cli/src/env.rs", "the one process-input reader");
const CONFIG: (&str, &str) = ("humanify-cli/src/config.rs", "the configuration owner");
const MAIN: (&str, &str) = (
    "humanify-cli/src/main.rs",
    "the binary's entry: argv in, and the stage verbs' clap declarations",
);
const SURFACE: (&str, &str) = (
    "humanify-cli/src/surface.rs",
    "the pipeline's clap declaration",
);
const INDEX_MAIN: (&str, &str) = (
    "humanify-index/src/main.rs",
    "the library-index builder: a separate binary with its own argv",
);

fn rules() -> Vec<Rule> {
    vec![
        Rule {
            name: "std::env / dotenv read outside the env module",
            hit: |l| {
                // A scratch-directory path convention, not a config read.
                !l.contains("std::env::temp_dir")
                    && (l.contains("std::env::")
                        || l.contains("env::var(")
                        || l.contains("dotenvy"))
            },
            owners: &[ENV_MODULE],
        },
        Rule {
            name: "process-input reader called outside the parse point",
            hit: |l| {
                l.contains("env::get(")
                    || l.contains("env::user_args(")
                    || l.contains("crate::env::")
            },
            owners: &[ENV_MODULE, CONFIG, MAIN],
        },
        Rule {
            name: "argument parser outside the parse point",
            hit: |l| l.contains("clap::") || l.contains("use clap"),
            owners: &[CONFIG, MAIN, SURFACE, INDEX_MAIN],
        },
        Rule {
            name: "config-file reader outside the config owner",
            hit: |l| l.contains("toml::") || l.contains("use toml"),
            owners: &[CONFIG],
        },
    ]
}

/// Every guarded (path, line number, line) in the workspace's sources.
fn guarded_lines() -> Vec<(String, usize, String)> {
    let crates_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut out = Vec::new();
    walk(&crates_dir, &mut |path, src| {
        let s = path.to_string_lossy().replace('\\', "/");
        if !s.contains("/src/") || s.contains("/tests/") || s.ends_with("_test.rs") {
            return;
        }
        let lines: Vec<&str> = src.lines().collect();
        let mut i = 0;
        while i < lines.len() {
            let trimmed = lines[i].trim_start();
            if trimmed.starts_with("#[cfg(test)]") {
                // `#[cfg(test)] mod x_test;` declares a test FILE (walked
                // and skipped on its own): skip just the declaration — code
                // after it is still guarded. Anything else (an inline
                // `mod tests { … }`, by convention the file's tail) ends
                // the guarded part.
                match lines.get(i + 1).map(|l| l.trim()) {
                    Some(next) if next.starts_with("mod ") && next.ends_with(';') => {
                        i += 2;
                        continue;
                    }
                    _ => break,
                }
            }
            if !trimmed.starts_with("//") {
                out.push((s.clone(), i + 1, lines[i].to_string()));
            }
            i += 1;
        }
    });
    out
}

fn offenders(
    rule_hit: impl Fn(&str) -> bool,
    owners: &[(&'static str, &'static str)],
) -> Vec<String> {
    guarded_lines()
        .into_iter()
        .filter(|(path, _, line)| rule_hit(line) && !owners.iter().any(|(o, _)| path.ends_with(o)))
        .map(|(path, n, line)| format!("{path}:{n}: {}", line.trim()))
        .collect()
}

/// Rule 5: a setting's name looked up in a JSON map. The names are the
/// config record's own keys (`humanify_cli::config::setting_keys`), so a
/// new setting is guarded the day it is added.
fn setting_lookup_offenders() -> Vec<String> {
    let mut names: Vec<String> = humanify_cli::config::setting_keys()
        .iter()
        .map(|k| k.rsplit('.').next().unwrap_or(k).to_string())
        .collect();
    names.sort();
    names.dedup();
    let patterns: Vec<String> = names
        .iter()
        .flat_map(|n| {
            [
                format!("[\"{n}\"]"),
                format!(".get(\"{n}\")"),
                format!(".bool(\"{n}\")"),
                format!(".str(\"{n}\")"),
                format!(".num(\"{n}\")"),
            ]
        })
        .collect();
    offenders(
        |line| patterns.iter().any(|p| line.contains(p.as_str())),
        &[CONFIG],
    )
}

#[test]
fn no_setting_is_read_back_out_of_a_json_map() {
    let found = setting_lookup_offenders();
    assert!(
        found.is_empty(),
        "a setting looked up by name in a JSON map outside the config owner — \
         read the typed Config (docs/config-consolidation.md §3.5):\n  {}",
        found.join("\n  ")
    );
}

#[test]
fn the_guard_examines_sources() {
    assert!(
        guarded_lines().len() > 10_000,
        "the guard examined (almost) nothing — the walk is broken"
    );
}

#[test]
fn configuration_is_read_only_by_its_owner() {
    let mut failures = Vec::new();
    for rule in rules() {
        let found = offenders(rule.hit, rule.owners);
        if !found.is_empty() {
            failures.push(format!("{}:\n  {}", rule.name, found.join("\n  ")));
        }
    }
    assert!(
        failures.is_empty(),
        "configuration read outside its owner (docs/config-consolidation.md §3.5) — \
         read the typed Config instead:\n{}",
        failures.join("\n")
    );
}

fn walk(dir: &Path, f: &mut impl FnMut(&Path, &str)) {
    let mut entries: Vec<std::fs::DirEntry> = match std::fs::read_dir(dir) {
        Ok(rd) => rd.filter_map(|e| e.ok()).collect(),
        Err(_) => return,
    };
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let path = dir.join(entry.file_name());
        if entry.file_name() == "target" {
            continue;
        }
        if path.is_dir() {
            walk(&path, f);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            let src = std::fs::read_to_string(&path).unwrap_or_default();
            f(&path, &src);
        }
    }
}
