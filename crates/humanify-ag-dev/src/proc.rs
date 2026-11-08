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

/// How a [`run_to_file`] child ended.
#[derive(Debug)]
pub struct LoggedExit {
    /// The exit code; -1 when killed by a signal (run-pipeline.ts's
    /// `code ?? -1`).
    pub code: i32,
}

/// Run `spawn` with its stdout and stderr both appended to `log` (a
/// pipeline launch's `-vv` output can be gigabytes: it goes to a file,
/// never into memory, and with no pipe there is nothing to drain), inside
/// its deadline. `on_tick(pid)` is called once right after the spawn and
/// then every `tick` while the child lives — an OBSERVER (rule 8): the
/// scored leg samples the process tree's peak RSS there, which can only be
/// read while the child is alive. The wait is the child's exit event on a
/// bounded channel with `recv_timeout` (never a sleep-and-poll); on expiry
/// the child's process group is killed.
pub fn run_to_file(
    spawn: &Spawn,
    log: &Path,
    tick: Duration,
    on_tick: &mut dyn FnMut(u32),
) -> Result<LoggedExit, String> {
    use std::os::unix::process::CommandExt;
    use std::sync::mpsc;
    let file = std::fs::File::create(log).map_err(|e| format!("{}: {e}", log.display()))?;
    let err = file
        .try_clone()
        .map_err(|e| format!("{}: {e}", log.display()))?;
    let mut command = Command::new(&spawn.program);
    command
        .args(&spawn.args)
        .process_group(0)
        .stdin(std::process::Stdio::null())
        .stdout(file)
        .stderr(err);
    if let Some(dir) = &spawn.cwd {
        command.current_dir(dir);
    }
    for (k, v) in &spawn.env {
        command.env(k, v);
    }
    let started = std::time::Instant::now();
    let mut child = command
        .spawn()
        .map_err(|e| format!("{}: cannot start: {e}", spawn.describe()))?;
    let pid = child.id();
    on_tick(pid);
    // concurrency: the one waiter of a file-logged child; it reports the
    // exit on a bounded channel and ends with the child.
    let (tx, rx) = mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let _ = tx.send(child.wait());
    });
    loop {
        let left = spawn.deadline.saturating_sub(started.elapsed());
        if left.is_zero() {
            kill_group(pid);
            return Err(format!(
                "{}: timed out after {}s (killed)",
                spawn.describe(),
                spawn.deadline.as_secs()
            ));
        }
        match rx.recv_timeout(left.min(tick)) {
            Ok(Ok(status)) => {
                return Ok(LoggedExit {
                    code: status.code().unwrap_or(-1),
                });
            }
            Ok(Err(e)) => return Err(format!("{}: wait: {e}", spawn.describe())),
            Err(mpsc::RecvTimeoutError::Timeout) => on_tick(pid),
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(format!("{}: the waiter died", spawn.describe()));
            }
        }
    }
}

/// Kill a child's whole process group (`kill -KILL -- -<pgid>`; the
/// harness forbids no unsafe, but the one owner of this already shells out).
fn kill_group(pid: u32) {
    let _ = humanify_ag_core::subprocess::run(
        Command::new("kill")
            .arg("-KILL")
            .arg("--")
            .arg(format!("-{pid}")),
        None,
        Duration::from_secs(10),
    );
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
