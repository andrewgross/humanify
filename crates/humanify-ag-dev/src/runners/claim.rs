//! A live run's claim on a folder it writes (exp205). There is no
//! host-wide lock: the coordinating agent decides what runs together from
//! each verb's footprint (`cargo dev` prints it). What is left is the
//! ACCIDENT — two runs writing one folder — and that is refused, never
//! queued: a second run on a claimed folder fails at once, naming the run
//! that holds it. Nothing ever waits.
//!
//! The claim is a marker file BESIDE the folder (`<folder>.live-run`; never
//! inside it: a cache or store reader lists its folder), created
//! exclusively, holding the run's pid, its start time (from
//! `/proc/<pid>/stat`, so a reused pid is not mistaken for the holder) and
//! what it is. The run removes it when the claim drops. A marker whose
//! holder is no longer alive — a killed run — is taken over, so a crash
//! blocks nobody.
//!
//! Not a flock on purpose: a flock lives on an open file, and a child
//! forked in another thread holds a copy until it execs, so a run that
//! released its claim could still read as held for a moment.

use std::io::Write;
use std::path::{Path, PathBuf};

/// A held claim; dropping it removes the marker.
#[derive(Debug)]
pub struct Claim {
    marker: PathBuf,
    stamp: String,
}

impl Drop for Claim {
    fn drop(&mut self) {
        // only our own marker: never one a later run took over
        if std::fs::read_to_string(&self.marker).is_ok_and(|t| t == self.stamp) {
            let _ = std::fs::remove_file(&self.marker);
        }
    }
}

/// The marker of `folder`: `<folder>.live-run`, beside it.
pub fn marker_of(folder: &Path) -> PathBuf {
    let mut name = folder
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_default();
    name.push(".live-run");
    folder.with_file_name(name)
}

/// Claim `folder` for this run (`what`: the verb and its subject, e.g.
/// "eval score <label>"). Err names the live run that holds it.
pub fn claim_folder(folder: &Path, what: &str) -> Result<Claim, String> {
    claim(&marker_of(folder), folder, what)
}

/// Claim through an explicit `marker`, guarding `guards` (named in the
/// refusal). Err when a live run holds the marker.
pub fn claim(marker: &Path, guards: &Path, what: &str) -> Result<Claim, String> {
    if let Some(dir) = marker.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let pid = std::process::id();
    let start = start_time(pid).unwrap_or_default();
    let stamp = format!("pid {pid} start {start} — {what}\n");
    // twice: the second attempt follows taking over a dead run's marker
    for _ in 0..2 {
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(marker)
        {
            Ok(mut f) => {
                f.write_all(stamp.as_bytes())
                    .map_err(|e| format!("{}: {e}", marker.display()))?;
                return Ok(Claim {
                    marker: marker.to_path_buf(),
                    stamp,
                });
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                let holder = std::fs::read_to_string(marker).unwrap_or_default();
                if holder_alive(&holder) {
                    return Err(refusal(marker, guards, &holder));
                }
                let _ = std::fs::remove_file(marker);
            }
            Err(e) => return Err(format!("{}: {e}", marker.display())),
        }
    }
    let holder = std::fs::read_to_string(marker).unwrap_or_default();
    Err(refusal(marker, guards, &holder))
}

fn refusal(marker: &Path, guards: &Path, holder: &str) -> String {
    format!(
        "{} is being written by another live run ({}; marker {}) — give this run its own folder, or run it after that one (delete the marker only if no such run is alive)",
        guards.display(),
        holder.trim(),
        marker.display()
    )
}

/// Whether a marker's holder still runs: its pid is alive with the start
/// time it recorded. A marker being written (empty) or a host without
/// `/proc` reads as alive — refusing is the safe answer.
fn holder_alive(holder: &str) -> bool {
    let mut words = holder.split_whitespace();
    let (Some("pid"), Some(pid), Some("start"), Some(start)) =
        (words.next(), words.next(), words.next(), words.next())
    else {
        return true;
    };
    let Ok(pid) = pid.parse::<u32>() else {
        return true;
    };
    if start_time(std::process::id()).is_none() {
        return true;
    }
    start_time(pid).is_some_and(|s| s == start)
}

/// A process's start time (`/proc/<pid>/stat` field 22, clock ticks since
/// boot); None when it is not running or there is no `/proc`.
fn start_time(pid: u32) -> Option<String> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // the command name (field 2) may hold spaces: count after its ')'
    let rest = &stat[stat.rfind(')')? + 1..];
    rest.split_whitespace().nth(19).map(str::to_string)
}
