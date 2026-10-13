//! The ONE module that reads the process environment (02 §2, 02 §3).
//!
//! TS original: `src/env.ts` (8 lines) — `env(name, fallback)` over
//! `process.env`, with a dotenv file loaded once at module load. The Rust
//! port keeps the shape and the guard: every other module in the workspace
//! receives environment-derived values as CONFIG; a `std::env` read outside
//! this module is a gate failure (the kill-switch guard test walks the
//! sources for it, and clippy's `disallowed_methods` is the static layer).
//!
//! History carried over verbatim: generation 1 of the kill switches WERE
//! env vars read inline at 14 sites with three incompatible predicates;
//! generation 2 (2026-08-12) deleted the env vars entirely. The env module
//! exists for exactly one read today — the API key, `HUMANIFY_API_KEY`,
//! asked for once by the configuration owner (`crate::config`; the
//! `OPENAI_API_KEY` fallback and `--api-key` were retired 2026-10-13) — and
//! it is the only module allowed to grow another. `tests/env_guard.rs`
//! fails a read anywhere else, and a call into this module from anywhere
//! but the parse point.

use std::sync::OnceLock;

static DOTENV_LOADED: OnceLock<()> = OnceLock::new();

/// Load a `.env` file from the working directory, once per process (the TS
/// module-load `dotenv.config()`; a missing file is not an error, exactly
/// as dotenv treats it). Called by every `get`, idempotent by OnceLock.
fn load_dotenv() {
    DOTENV_LOADED.get_or_init(|| {
        let _ = dotenvy::dotenv();
    });
}

/// Read one environment variable, with an optional fallback (env.ts:31-34).
/// The allow is the point: this module is the ONE reader the lint forbids
/// everywhere else (the guard test is the dynamic layer, this is static).
// env: the one environment reader (WP1.1); see the doc comment above.
#[allow(clippy::disallowed_methods)]
pub fn get(name: &str, fallback: Option<&str>) -> Option<String> {
    load_dotenv();
    match std::env::var(name) {
        Ok(v) => Some(v),
        Err(_) => fallback.map(|f| f.to_string()),
    }
}

/// The user's argv (no program name). Not an environment variable, but it
/// is process input, and this module is the one place process inputs are
/// read (the env guard test enforces `std::env` nowhere else).
pub fn user_args() -> Vec<String> {
    std::env::args().skip(1).collect()
}
