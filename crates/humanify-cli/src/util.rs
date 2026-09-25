//! The small shared utilities of WP1.1's contents column, one file each in
//! TS (`number-utils.ts`, `file-utils.ts`, `shared/regex.ts`,
//! `utils/identifier-regex.ts`, `commands/default-args.ts`). The concurrency
//! limiter is NOT here: it is a pipeline utility (its consumers are the
//! processor and the sweep), so it ports into `humanify-core` with them.

use std::path::{Path, PathBuf};

/// Parse a numeric flag value exactly as the TS `parseNumber`
/// (number-utils.ts): `parseInt(value, 10)` — skip leading JS whitespace,
/// an optional sign, then the longest run of ASCII digits; no digits is
/// NaN, and NaN is an error, never a silent default. The result is the JS
/// number (so `12abc` is 12 and `1e3` is 1, as the TS resolves them).
pub fn parse_number(value: &str) -> Result<f64, String> {
    let rest = value.trim_start_matches(humanify_model::js::is_js_whitespace);
    let (negative, rest) = match rest.as_bytes().first() {
        Some(b'-') => (true, &rest[1..]),
        Some(b'+') => (false, &rest[1..]),
        _ => (false, rest),
    };
    let digits: &str = &rest[..rest.bytes().take_while(u8::is_ascii_digit).count()];
    if digits.is_empty() {
        return Err(format!("Invalid number: {value}"));
    }
    let magnitude: f64 = digits.parse().expect("ASCII digits parse as f64");
    Ok(if negative { -magnitude } else { magnitude })
}

/// Crash loudly with a red message on stderr (cli-error.ts:1-4).
pub fn err(message: &str) -> ! {
    eprintln!("\x1b[31m{message}\x1b[0m");
    std::process::exit(1);
}

/// Crash loudly with a red message and an explicit exit code.
pub fn err_with_code(message: &str, code: i32) -> ! {
    eprintln!("\x1b[31m{message}\x1b[0m");
    std::process::exit(code);
}

/// The input file must exist; a missing one is a loud failure
/// (file-utils.ts:4-8).
pub fn ensure_file_exists(filename: &Path) {
    if !filename.exists() {
        err(&format!("File {} not found", filename.display()));
    }
}

/// Recursively list JS files under `dir`, relative to `rootDir`, skipping
/// `node_modules` AND `.humanify/` (pipeline metadata — the split ledger
/// and `humanified.js` double-count the tree's code; file-utils.ts:20-38).
/// Directory reads SORT before use (02 §5: host enumeration order never
/// reaches a decision).
pub fn list_js_files_recursive(dir: &Path, root_dir: &Path, exts: &[&str]) -> Vec<PathBuf> {
    let mut results = Vec::new();
    let mut entries: Vec<std::fs::DirEntry> = match std::fs::read_dir(dir) {
        Ok(rd) => rd.filter_map(|e| e.ok()).collect(),
        Err(_) => return results,
    };
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let full = dir.join(entry.file_name());
        if full.is_dir() {
            let name = entry.file_name();
            if name == "node_modules" || name == ".humanify" {
                continue;
            }
            results.extend(list_js_files_recursive(&full, root_dir, exts));
        } else if exts
            .iter()
            .any(|e| entry.file_name().to_string_lossy().ends_with(e))
        {
            results.push(full.strip_prefix(root_dir).unwrap_or(&full).to_path_buf());
        }
    }
    results
}

/// Escape a string for literal use inside a regex pattern — the single
/// owner of the question (shared/regex.ts; copied verbatim in three files
/// before 2026-08-10). Note: for Rust's `regex` crate the escapes differ
/// (`$` is NOT special in Rust regex patterns outside groups); this ports
/// the ECMA-RegExp semantics because its consumers render patterns for
/// cross-implementation comparison. Consumer crates decide.
pub fn escape_regexp(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        if matches!(
            c,
            '.' | '*' | '+' | '?' | '^' | '$' | '{' | '}' | '(' | ')' | '|' | '[' | ']' | '\\'
        ) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Whole-identifier matching pattern (utils/identifier-regex.ts): lookarounds
/// over the identifier charset, because `\b` is wrong for minified names
/// containing `$` (not a regex word character). Returns the PATTERN STRING;
/// regex flags are the consumer engine's concern (TS passes them as
/// `new RegExp(pattern, flags)` — a second argument, not pattern content).
pub fn identifier_regex_pattern(name: &str) -> String {
    let escaped = name.replace('$', "\\$");
    format!("(?<![A-Za-z0-9_$]){escaped}(?![A-Za-z0-9_$])")
}

/// The run defaults (commands/default-args.ts) — one source each.
pub const DEFAULT_CONCURRENCY: u32 = 50;

/// LLM request timeout. One number, because it was three (the CLI default,
/// the provider fallback, and a hard-coded experiment copy).
pub const DEFAULT_LLM_TIMEOUT_MS: u64 = 300_000;

/// Module-lane concurrency, per bundler. esbuild bundles hold many more
/// independent module bindings, so their lane is wider.
pub fn default_module_concurrency(bundler_type: Option<&str>) -> u32 {
    if bundler_type == Some("esbuild") {
        MODULE_LANES_ESBUILD
    } else {
        MODULE_LANES_OTHER
    }
}

const MODULE_LANES_ESBUILD: u32 = 40;
const MODULE_LANES_OTHER: u32 = 20;

/// The widest lane `default_module_concurrency` can return — the LLM rate
/// limiter's outer bound, sized before the bundler is known. DERIVED from
/// the lane table rather than restated, so raising a lane cannot leave the
/// ceiling behind.
pub const MAX_DEFAULT_MODULE_CONCURRENCY: u32 = if MODULE_LANES_ESBUILD > MODULE_LANES_OTHER {
    MODULE_LANES_ESBUILD
} else {
    MODULE_LANES_OTHER
};
