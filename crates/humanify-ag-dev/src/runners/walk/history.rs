//! Package a finished (or partial) walk as a git history: one commit + one
//! tag per version, in walk order (the port of `experiments/walk/history.sh`),
//! so `git --git-dir <walk>/history.git diff v2.1.215 v2.1.216 -- src/`
//! shows exactly the diff a human reviewing the release would read.
//!
//! vendor/ and the runnable scaffolding are KEPT (hiding vendor/ is how it
//! went unscored for thirteen experiments, rule 8); only `.humanify/` (the
//! bundle + ledger — metadata, not source), `node_modules/` and
//! `package-lock.json` are excluded. Only FINISHED hops (a
//! `runs/<v>.hop.json`) are committed. Commits carry a fixed date, so
//! rebuilding the same walk gives the same SHAs.

use std::path::{Path, PathBuf};
use std::time::Duration;

use super::super::{Ctx, remove, text};
use crate::proc::{Spawn, run_ok};

const GIT_DEADLINE: Duration = Duration::from_secs(3600);
const DATE: &str = "2000-01-01T00:00:00Z";

fn git(repo: &Path, index: &Path, work: Option<&Path>, args: &[&str]) -> Result<String, String> {
    let mut env = vec![
        ("GIT_DIR".to_string(), text(repo)),
        ("GIT_INDEX_FILE".to_string(), text(index)),
        ("GIT_AUTHOR_NAME".to_string(), "humanify walk".to_string()),
        (
            "GIT_AUTHOR_EMAIL".to_string(),
            "walk@humanify.invalid".to_string(),
        ),
        (
            "GIT_COMMITTER_NAME".to_string(),
            "humanify walk".to_string(),
        ),
        (
            "GIT_COMMITTER_EMAIL".to_string(),
            "walk@humanify.invalid".to_string(),
        ),
        ("GIT_AUTHOR_DATE".to_string(), DATE.to_string()),
        ("GIT_COMMITTER_DATE".to_string(), DATE.to_string()),
    ];
    if let Some(w) = work {
        env.push(("GIT_WORK_TREE".to_string(), text(w)));
    }
    let spawn = Spawn {
        program: "git".into(),
        args: args.iter().map(|a| a.to_string()).collect(),
        cwd: work.map(Path::to_path_buf),
        env,
        stdin: None,
        deadline: GIT_DEADLINE,
    };
    run_ok(&spawn)
}

/// Rebuild `<walk>/history.git` from the finished hops of `versions` (in
/// walk order); returns the repo.
pub fn build(
    ctx: &Ctx,
    walk: &Path,
    versions: &[String],
    pipeline: &str,
) -> Result<PathBuf, String> {
    let repo = walk.join("history.git");
    remove(&repo);
    let scratch = walk.join(".history-index");
    remove(&scratch);
    std::fs::create_dir_all(&scratch).map_err(|e| e.to_string())?;
    let index = scratch.join("index");
    crate::proc::git(
        walk,
        &["init", "--bare", "--quiet", "-b", "main", &text(&repo)],
    )?;
    let mut n = 0;
    for v in versions {
        let tree = walk.join("trees").join(v);
        if !walk.join("runs").join(format!("{v}.hop.json")).is_file() || !tree.is_dir() {
            ctx.say(format!("  (stop at {v}: hop not finished)"));
            break;
        }
        git(
            &repo,
            &index,
            Some(&tree),
            &[
                "add",
                "-A",
                "--",
                ".",
                ":(exclude).humanify",
                ":(exclude)node_modules",
                ":(exclude)package-lock.json",
            ],
        )?;
        let msg = format!("claude-code v{v} (humanify {pipeline} walk)");
        git(
            &repo,
            &index,
            Some(&tree),
            &["commit", "--quiet", "--allow-empty", "-m", &msg],
        )?;
        git(
            &repo,
            &index,
            Some(&tree),
            &[
                "tag",
                "-a",
                &format!("v{v}"),
                "-m",
                &format!("claude-code v{v}"),
            ],
        )?;
        n += 1;
    }
    remove(&scratch);
    ctx.say(format!("history: {n} commits -> {}", text(&repo)));
    Ok(repo)
}

/// `git diff -U2 --no-renames v<from> v<to> -- <scope>` in the history.
pub fn diff(repo: &Path, from: &str, to: &str, scope: &str) -> Result<String, String> {
    let spawn = Spawn {
        program: "git".into(),
        args: [
            "--git-dir",
            &text(repo),
            "diff",
            "-U2",
            "--no-renames",
            &format!("v{from}"),
            &format!("v{to}"),
            "--",
            scope,
        ]
        .iter()
        .map(|s| s.to_string())
        .collect(),
        cwd: None,
        env: Vec::new(),
        stdin: None,
        deadline: GIT_DEADLINE,
    };
    run_ok(&spawn)
}
