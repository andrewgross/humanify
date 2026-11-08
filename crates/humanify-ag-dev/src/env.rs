//! The harness's ONE read of the process environment (docs/dev-harness.md
//! §2.1), the twin of `humanify-ag-cli::env` for the pipeline: argv, the
//! two variables the harness legitimately needs from the host, and the
//! working directory. Everything else a run depends on is a setting
//! (`crate::settings`, flags parsed upfront) or a committed file
//! (`crate::committed`) — the ambient env-var configuration of the TS
//! harness is gone, and `tests/env_guard.rs` (humanify-ag-cli) fails a
//! `std::env` read anywhere else in this crate.

use std::path::PathBuf;

/// What the harness reads from the host environment, read once.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HostEnv {
    /// `$HOME`: where the user-level cargo (`~/.cargo/bin`) and bun
    /// (`~/.bun/bin`) live — shells that never sourced ~/.profile lack
    /// them on PATH (the boot gate silently skipped without bun).
    pub home: Option<PathBuf>,
    /// `$PATH`, which a child process inherits with those two prepended.
    pub path: Option<String>,
    /// This binary's own path: the gate re-invokes it for its in-crate
    /// stages (`census concurrency`, `e2e`, ...).
    pub current_exe: Option<PathBuf>,
    /// The working directory, for resolving a relative path the user typed.
    pub cwd: Option<PathBuf>,
}

/// Read the host environment. The allow is the point: this function is
/// the one reader the lint forbids everywhere else.
// env: the harness's one environment reader; see the module doc.
#[allow(clippy::disallowed_methods)]
pub fn host() -> HostEnv {
    HostEnv {
        home: std::env::var_os("HOME").map(PathBuf::from),
        path: std::env::var("PATH").ok(),
        current_exe: std::env::current_exe().ok(),
        cwd: std::env::current_dir().ok(),
    }
}

/// The user's argv (no program name).
pub fn user_args() -> Vec<String> {
    std::env::args().skip(1).collect()
}

/// The working directory, for resolving a relative path the user typed.
pub fn current_dir() -> Result<PathBuf, String> {
    std::env::current_dir().map_err(|e| format!("cannot read the working directory: {e}"))
}

impl HostEnv {
    /// PATH for a child: `~/.cargo/bin` and `~/.bun/bin` first, then the
    /// inherited PATH.
    pub fn child_path(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if let Some(home) = &self.home {
            parts.push(home.join(".cargo/bin").display().to_string());
            parts.push(home.join(".bun/bin").display().to_string());
        }
        if let Some(p) = &self.path {
            parts.push(p.clone());
        }
        parts.join(":")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_child_path_puts_the_user_tools_first() {
        let env = HostEnv {
            home: Some(PathBuf::from("/h")),
            path: Some("/usr/bin:/bin".into()),
            ..HostEnv::default()
        };
        assert_eq!(env.child_path(), "/h/.cargo/bin:/h/.bun/bin:/usr/bin:/bin");
        assert_eq!(HostEnv::default().child_path(), "");
    }
}
