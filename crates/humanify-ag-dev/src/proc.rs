//! Every child process the harness starts goes through [`run`]: ONE
//! `Command::new` in the crate, on top of the workspace's one subprocess
//! owner (`humanify_ag_core::subprocess::run`: every pipe drained at once,
//! one deadline over the whole operation, the process group killed when it
//! expires, every failure returned). docs/concurrency.md rules 3 and 4;
//! the harness's many subprocesses are allowlisted ONCE, here (dev-harness
//! design §8 risk 8).

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

pub use humanify_ag_core::subprocess::Finished;

/// One child to run: program, arguments, and how.
#[derive(Clone, Debug)]
pub struct Spawn {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
    /// Variables set (or replaced) for the child; the rest is inherited.
    pub env: Vec<(String, String)>,
    pub stdin: Option<Vec<u8>>,
    /// The deadline over the WHOLE run (spawn → exit).
    pub deadline: Duration,
}

impl Spawn {
    pub fn new(program: &str, args: &[&str], deadline: Duration) -> Spawn {
        Spawn {
            program: program.to_string(),
            args: args.iter().map(|a| a.to_string()).collect(),
            cwd: None,
            env: Vec::new(),
            stdin: None,
            deadline,
        }
    }

    pub fn cwd(mut self, dir: &Path) -> Spawn {
        self.cwd = Some(dir.to_path_buf());
        self
    }

    pub fn env(mut self, key: &str, value: &str) -> Spawn {
        self.env.push((key.to_string(), value.to_string()));
        self
    }

    /// The command line as one string, for messages.
    pub fn describe(&self) -> String {
        std::iter::once(self.program.as_str())
            .chain(self.args.iter().map(String::as_str))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// The crate's ONE `Command::new`: `spawn` as a command, unstarted.
fn command(spawn: &Spawn) -> Command {
    let mut command = Command::new(&spawn.program);
    command.args(&spawn.args);
    if let Some(dir) = &spawn.cwd {
        command.current_dir(dir);
    }
    for (k, v) in &spawn.env {
        command.env(k, v);
    }
    command
}

/// Run `spawn` to completion within its deadline; any exit status comes
/// back for the caller to judge. Err when it could not start, timed out
/// (and was killed), or a pipe failed.
pub fn run(spawn: &Spawn) -> Result<Finished, String> {
    humanify_ag_core::subprocess::run(&mut command(spawn), spawn.stdin.clone(), spawn.deadline)
        .map_err(|e| format!("{}: {e}", spawn.describe()))
}

/// Run `spawn` with its stdout and stderr going straight to ours (a gate
/// stage: the person watches it as it happens), inside its deadline; the
/// exit status comes back. `stdin` must be None. Err as [`run`].
pub fn run_streamed(spawn: &Spawn) -> Result<std::process::ExitStatus, String> {
    if spawn.stdin.is_some() {
        return Err(format!(
            "{}: a streamed child takes no stdin",
            spawn.describe()
        ));
    }
    humanify_ag_core::subprocess::run_inherited(&mut command(spawn), spawn.deadline)
        .map_err(|e| format!("{}: {e}", spawn.describe()))
}

/// [`run`], requiring exit 0; stdout as text. A failure names the command
/// and the tail of its stderr.
pub fn run_ok(spawn: &Spawn) -> Result<String, String> {
    let done = run(spawn)?;
    if !done.status.success() {
        let stderr = String::from_utf8_lossy(&done.stderr);
        let tail: Vec<&str> = stderr.lines().rev().take(20).collect();
        let tail: Vec<&str> = tail.into_iter().rev().collect();
        return Err(format!(
            "{} exited {}:\n{}",
            spawn.describe(),
            done.status,
            tail.join("\n")
        ));
    }
    String::from_utf8(done.stdout).map_err(|_| format!("{}: stdout is not UTF-8", spawn.describe()))
}

/// The deadline for a quick `git` query.
pub const GIT_DEADLINE: Duration = Duration::from_secs(60);

/// `git -C <root> <args>`, trimmed stdout.
pub fn git(root: &Path, args: &[&str]) -> Result<String, String> {
    let mut all = vec!["-C".to_string(), root.display().to_string()];
    all.extend(args.iter().map(|a| a.to_string()));
    let spawn = Spawn {
        program: "git".into(),
        args: all,
        cwd: None,
        env: Vec::new(),
        stdin: None,
        deadline: GIT_DEADLINE,
    };
    run_ok(&spawn).map(|s| s.trim().to_string())
}

#[cfg(test)]
#[path = "proc_test.rs"]
mod proc_test;
