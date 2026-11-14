//! Every runner the harness launches (docs/dev-harness.md §5 lane L3):
//! `eval score`, `eval neutrality`, `eval walk`, the library runner's
//! discover launches, the boot gate, `index build`, `run stub` and
//! `build-bin <ref>`. Each one's launches are held byte-identical to the
//! committed launch goldens (proof (c)) by its `*_test.rs`, which runs it
//! for real with recorders in place of the binary and the tools it starts.
//!
//! A runner never reads the environment itself: what it needs from the
//! host comes in a [`Ctx`] (the checkout, HOME / PATH from `crate::env`,
//! and any extra child variables a test plants), and every child goes
//! through `crate::proc`.

pub mod batch;
pub mod bin;
pub mod boot;
pub mod claim;
pub mod index;
pub mod lanes;
pub mod leg;
pub mod library;
pub mod neutrality;
pub mod records;
pub mod score;
pub mod scoring;
pub mod stub;
pub mod tree_diff;
pub mod verbs;
pub mod walk;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::env;
use crate::proc::Spawn;

/// What a runner may know about where it runs.
pub struct Ctx {
    /// The checkout whose committed files (pairs.json, ...) the run reads
    /// and whose results tree it writes.
    pub repo: PathBuf,
    pub host: env::HostEnv,
    /// The working directory a relative path the user typed is read
    /// against.
    pub cwd: PathBuf,
    /// Variables every child gets on top of the inherited environment
    /// (a test's recorder root; nothing in production).
    pub child_env: Vec<(String, String)>,
    /// Echo every line to stdout as it is said (off in tests).
    pub echo: bool,
    said: Mutex<Vec<String>>,
    /// A lane's context says into its slot of the run's ordered
    /// transcript instead (`lanes`, exp207).
    lane: Option<(Arc<lanes::Lanes>, usize)>,
}

impl Ctx {
    pub fn new(repo: PathBuf, host: env::HostEnv, cwd: PathBuf) -> Ctx {
        Ctx {
            repo,
            host,
            cwd,
            child_env: Vec::new(),
            echo: true,
            said: Mutex::new(Vec::new()),
            lane: None,
        }
    }

    /// The context one lane of a parallel run works in: the same host,
    /// its lines said into `slot` of `lanes`.
    pub fn for_lane(&self, lanes: &Arc<lanes::Lanes>, slot: usize) -> Ctx {
        Ctx {
            repo: self.repo.clone(),
            host: self.host.clone(),
            cwd: self.cwd.clone(),
            child_env: self.child_env.clone(),
            echo: self.echo,
            said: Mutex::new(Vec::new()),
            lane: Some((Arc::clone(lanes), slot)),
        }
    }

    /// Lines the lanes already printed, added to this transcript in their
    /// (canonical) order without printing them again.
    pub fn absorb(&self, lines: Vec<String>) {
        self.said.lock().expect("the transcript").extend(lines);
    }

    /// `p` as typed, made absolute against the working directory.
    pub fn absolute(&self, p: &str) -> PathBuf {
        let path = Path::new(p);
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.cwd.join(path)
        }
    }

    /// Say one line (a runner's progress and verdict lines).
    pub fn say(&self, line: impl Into<String>) {
        let line = line.into();
        if let Some((lanes, slot)) = &self.lane {
            lanes.say(*slot, line);
            return;
        }
        if self.echo {
            println!("{line}");
        }
        self.said.lock().expect("the transcript").push(line);
    }

    /// Everything said so far, one line each.
    pub fn transcript(&self) -> String {
        self.said.lock().expect("the transcript").join("\n")
    }

    /// A child: `program args` with PATH = the host's child PATH (the
    /// user-level cargo and bun first) and the context's extra variables.
    pub fn spawn(&self, program: &str, args: &[String], deadline: Duration) -> Spawn {
        let mut s = Spawn {
            program: program.to_string(),
            args: args.to_vec(),
            cwd: None,
            env: Vec::new(),
            stdin: None,
            deadline,
        };
        s.env.push(("PATH".into(), self.host.child_path()));
        s.env.extend(self.child_env.iter().cloned());
        s
    }

    /// Build the pipeline at the checkout `root` (cargo runs with this
    /// context's child environment).
    pub fn build(&self, root: &Path) -> Result<crate::bin_build::BinRecord, String> {
        let mut env = vec![("PATH".to_string(), self.host.child_path())];
        env.extend(self.child_env.iter().cloned());
        crate::bin_build::build_with_env(root, &env)
    }

    /// The repo-relative path of a committed file.
    pub fn repo_path(&self, rel: &str) -> PathBuf {
        self.repo.join(rel)
    }
}

/// `path` as text (every launch line and record spells paths as text).
pub fn text(path: &Path) -> String {
    path.display().to_string()
}

/// Write `text` to `path`, creating its directory.
pub fn write(path: &Path, text: &str) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
}

/// Remove a file or a directory tree if it exists.
pub fn remove(path: &Path) {
    if path.is_dir() {
        let _ = std::fs::remove_dir_all(path);
    } else {
        let _ = std::fs::remove_file(path);
    }
}

/// How many files are under `dir` (an LLM cache's entry count; 0 when
/// absent).
pub fn count_files(dir: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    entries
        .filter_map(Result::ok)
        .map(|e| {
            let p = e.path();
            if p.is_dir() { count_files(&p) } else { 1 }
        })
        .sum()
}

/// Run `f` over `items`, `jobs` at a time, on scoped threads; the results
/// come back in INPUT order (completion order reaches nothing: rule 6).
/// For subprocess fan-outs only (a discover run per bundle, an index
/// chunk, a walk hop's scoring): each `f` drives one child through the one
/// subprocess owner. CPU parallelism goes through `humanify_ag_core::par`.
pub fn each<T: Sync, R: Send>(items: &[T], jobs: usize, f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let next = Mutex::new(0usize);
    let done: Mutex<Vec<Option<R>>> = Mutex::new((0..items.len()).map(|_| None).collect());
    // concurrency: `jobs` scoped workers, each taking the next item and
    // running its one child to completion; joined before return.
    std::thread::scope(|scope| {
        for _ in 0..jobs.clamp(1, items.len().max(1)) {
            scope.spawn(|| {
                loop {
                    let i = {
                        let mut n = next.lock().expect("the work index");
                        let i = *n;
                        *n += 1;
                        i
                    };
                    let Some(item) = items.get(i) else { return };
                    let r = f(item);
                    done.lock().expect("the results")[i] = Some(r);
                }
            });
        }
    });
    done.into_inner()
        .expect("the results")
        .into_iter()
        .map(|r| r.expect("every item ran"))
        .collect()
}

/// Copy a directory tree (`cp -r`).
pub fn copy_tree(from: &Path, to: &Path) -> Result<(), String> {
    std::fs::create_dir_all(to).map_err(|e| format!("{}: {e}", to.display()))?;
    let entries = std::fs::read_dir(from).map_err(|e| format!("{}: {e}", from.display()))?;
    for e in entries {
        let e = e.map_err(|e| e.to_string())?;
        let (src, dst) = (e.path(), to.join(e.file_name()));
        if src.is_dir() {
            copy_tree(&src, &dst)?;
        } else {
            std::fs::copy(&src, &dst).map_err(|e| format!("{}: {e}", src.display()))?;
        }
    }
    Ok(())
}

/// The input bundle of a walked Claude Code version: the decompile's
/// `src/entrypoints/index.js`, or, from 2.1.229 (where the decompile writes
/// the bundle at its root), `binary-decompiled/index.js`. The older path is
/// the answer whenever the root one is absent, so a version with neither is
/// reported at the path every version before 2.1.229 uses.
pub fn input_of(inputs_base: &str, version: &str) -> String {
    let dir = format!("{inputs_base}/claude-code-{version}/binary-decompiled");
    let entry = format!("{dir}/src/entrypoints/index.js");
    let root = format!("{dir}/index.js");
    if !Path::new(&entry).is_file() && Path::new(&root).is_file() {
        return root;
    }
    entry
}

/// The full sha (or short, with `short`) of a checkout's HEAD; "" when
/// unknown.
pub fn head(repo: &Path, short: bool) -> String {
    let args: &[&str] = if short {
        &["rev-parse", "--short", "HEAD"]
    } else {
        &["rev-parse", "HEAD"]
    };
    crate::proc::git(repo, args).unwrap_or_default()
}

/// Whether a checkout has uncommitted changes to tracked files.
pub fn dirty(repo: &Path) -> bool {
    crate::proc::git(repo, &["status", "--porcelain", "--untracked-files=no"])
        .map(|s| !s.is_empty())
        .unwrap_or(false)
}

#[cfg(test)]
mod claim_test;

#[cfg(test)]
mod recording_test;

#[cfg(test)]
mod score_test;

#[cfg(test)]
mod neutrality_test;

#[cfg(test)]
mod tree_diff_test;

#[cfg(test)]
mod library_test;

#[cfg(test)]
mod walk_test;

#[cfg(test)]
mod index_test;

#[cfg(test)]
mod stub_test;

#[cfg(test)]
mod config_test;
