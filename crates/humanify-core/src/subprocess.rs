//! The pipeline's one way to run a child process (docs/concurrency.md,
//! rule 4): every pipe drained at the same time, ONE deadline over the
//! whole operation (spawn → stdin written → output read → exit), the child
//! killed when it expires, and every failure returned as an error the
//! caller has to say out loud.
//!
//! Three callers: the webcrack shim, `diff` for the normal-diff hunks and
//! `git rev-parse` for the dump's manifest. Each picks its own deadline
//! (see their constants and docs/concurrency.md, "Subprocesses").
//!
//! How the wait works: the child goes into its OWN process group, and one
//! plain thread each drains stdout, drains stderr, feeds stdin and waits
//! for the exit. They report on one bounded channel, and the caller's
//! thread waits on that channel with `recv_timeout` until the deadline — an
//! event wait, never a sleep-and-poll. On expiry the whole group is killed
//! (`kill -KILL -- -<pgid>`): the shim is `npx tsx …`, whose grandchild
//! `node` holds the pipes open, so killing the direct child alone would
//! leave the drains blocked. The threads are not scoped on purpose: if
//! something outside the group still holds a pipe after the kill, the
//! threads end when it closes, and the run does not wait for them.

use std::fmt;
use std::io::{Read, Write};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// A child that ran to completion inside its deadline (any exit status:
/// what a status means is the caller's call).
#[derive(Debug)]
pub struct Finished {
    pub status: ExitStatus,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

/// Why a child did not finish.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubprocessError {
    /// It never started (not found, EAGAIN, ENOMEM, …).
    Spawn(String),
    /// The deadline expired; `killed` says whether the group kill was
    /// delivered.
    TimedOut { after: Duration, killed: bool },
    /// A pipe or the wait failed.
    Io(String),
}

impl fmt::Display for SubprocessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SubprocessError::Spawn(e) => write!(f, "cannot start: {e}"),
            SubprocessError::TimedOut { after, killed } => write!(
                f,
                "timed out after {}s ({})",
                after.as_secs_f64(),
                if *killed {
                    "killed"
                } else {
                    "the kill FAILED — the child may still be running"
                }
            ),
            SubprocessError::Io(e) => write!(f, "{e}"),
        }
    }
}

enum Event {
    Stdout(std::io::Result<Vec<u8>>),
    Stderr(std::io::Result<Vec<u8>>),
    Stdin(std::io::Result<()>),
    Exited(std::io::Result<ExitStatus>),
}

/// What the waits have collected so far.
#[derive(Default)]
struct Collected {
    stdout: Option<Vec<u8>>,
    stderr: Option<Vec<u8>>,
    status: Option<ExitStatus>,
    stdin_error: Option<String>,
    error: Option<String>,
}

impl Collected {
    fn take(&mut self, event: Event) {
        let pipe = |r: std::io::Result<Vec<u8>>, what: &str, error: &mut Option<String>| {
            r.unwrap_or_else(|e| {
                error.get_or_insert(format!("read {what}: {e}"));
                Vec::new()
            })
        };
        match event {
            Event::Stdout(r) => self.stdout = Some(pipe(r, "stdout", &mut self.error)),
            Event::Stderr(r) => self.stderr = Some(pipe(r, "stderr", &mut self.error)),
            Event::Stdin(r) => self.stdin_error = r.err().map(|e| format!("write stdin: {e}")),
            Event::Exited(r) => match r {
                Ok(status) => self.status = Some(status),
                Err(e) => {
                    self.error.get_or_insert(format!("wait: {e}"));
                }
            },
        }
    }
}

/// Run `command` to completion within `deadline`, writing `stdin` (when
/// given) while stdout and stderr drain; stdin is null when `None`. The
/// exit status is returned for the caller to judge. A failed stdin write
/// is an error only when the child still exited 0 — it left input unread
/// and claimed success; a child that failed reports through its status.
pub fn run(
    command: &mut Command,
    stdin: Option<Vec<u8>>,
    deadline: Duration,
) -> Result<Finished, SubprocessError> {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    command
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let started = Instant::now();
    let mut child = command
        .spawn()
        .map_err(|e| SubprocessError::Spawn(e.to_string()))?;
    let pid = child.id();
    // One slot per sender: nobody ever blocks on a send.
    let (tx, rx) = mpsc::sync_channel::<Event>(4);
    let mut pending = 0usize;
    if let Some(mut out) = child.stdout.take() {
        pending += 1;
        let tx = tx.clone();
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            let r = out.read_to_end(&mut buf).map(|_| buf);
            let _ = tx.send(Event::Stdout(r));
        });
    }
    if let Some(mut err) = child.stderr.take() {
        pending += 1;
        let tx = tx.clone();
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            let r = err.read_to_end(&mut buf).map(|_| buf);
            let _ = tx.send(Event::Stderr(r));
        });
    }
    if let (Some(bytes), Some(mut pipe)) = (stdin, child.stdin.take()) {
        pending += 1;
        let tx = tx.clone();
        // Dropping `pipe` at the end closes it: the child sees EOF.
        std::thread::spawn(move || {
            let _ = tx.send(Event::Stdin(pipe.write_all(&bytes)));
        });
    }
    pending += 1;
    std::thread::spawn(move || {
        let _ = tx.send(Event::Exited(child.wait()));
    });
    let mut got = Collected::default();
    while pending > 0 {
        let left = deadline.saturating_sub(started.elapsed());
        match rx.recv_timeout(left) {
            Ok(event) => {
                got.take(event);
                pending -= 1;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                return Err(SubprocessError::TimedOut {
                    after: deadline,
                    killed: kill_group(pid),
                });
            }
            // Every sender gone with events missing: a drain thread
            // panicked (it cannot otherwise exit without sending).
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                kill_group(pid);
                return Err(SubprocessError::Io("a pipe thread died".to_string()));
            }
        }
    }
    if let Some(e) = got.error {
        return Err(SubprocessError::Io(e));
    }
    let status = got.status.expect("the exit event arrived");
    if let Some(e) = got.stdin_error
        && status.success()
    {
        return Err(SubprocessError::Io(e));
    }
    Ok(Finished {
        status,
        stdout: got.stdout.unwrap_or_default(),
        stderr: got.stderr.unwrap_or_default(),
    })
}

/// SIGKILL the child's whole process group (it is the group's leader, so
/// the group id is its pid). Through `kill(1)`: core forbids `unsafe`, so
/// no direct `kill(2)`. `kill` returns at once, so it needs no deadline of
/// its own.
fn kill_group(pid: u32) -> bool {
    Command::new("kill")
        .args(["-KILL", "--", &format!("-{pid}")])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

#[cfg(test)]
mod subprocess_test;
