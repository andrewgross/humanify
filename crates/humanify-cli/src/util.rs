//! The small shared utilities of WP1.1's contents column, one file each in
//! TS (`file-utils.ts`, `shared/regex.ts`,
//! `utils/identifier-regex.ts`, `commands/default-args.ts`). The concurrency
//! limiter is NOT here: it is a pipeline utility (its consumers are the
//! processor and the sweep), so it ports into `humanify-core` with them.

use std::path::Path;

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
