//! WHICH BINARY a run launches, and which commit it was built from: the
//! label binary of `eval score` and `eval walk` (the port of
//! `experiments/lib/pipeline-bin.ts`) and `build-bin <ref>` (any commit,
//! in a detached worktree — the neutrality baseline, a walk at a ref).
//! Every build goes through `crate::bin_build` (the package and binary
//! name read from the checkout being built: `humanify-ag` since exp186,
//! `humanify` before, refused before the cutover).
//!
//! WHY THE HARNESS BUILDS IT. A binary is a file, and a file carries no
//! commit. A label records `commit.txt` = the repo's HEAD; a leftover
//! `target/release/humanify-ag` from another branch would make that a lie
//! nobody could detect afterwards. Building it here makes the label's
//! commit the binary's commit by construction, or refuses, loudly.

use std::path::{Path, PathBuf};

use super::leg::LabelBin;
use super::{Ctx, text};
use crate::bin_build::{self, same_commit};
use crate::proc::git;

/// The cargo workspace root that owns `<root>/target/<profile>/<bin>`, or
/// None when the binary sits in no workspace's target dir (a copied binary
/// has no build this harness can reproduce or date).
pub fn workspace_root_of(bin: &Path) -> Option<PathBuf> {
    let target = bin.parent()?.parent()?;
    if target.file_name()? != "target" {
        return None;
    }
    let root = target.parent()?;
    root.join("Cargo.toml").exists().then(|| root.to_path_buf())
}

/// Build `bin` where its workspace says (when one owns it), then describe
/// what was built.
pub fn build_and_describe(ctx: &Ctx, bin: &Path) -> Result<LabelBin, String> {
    let root = workspace_root_of(bin);
    if let Some(root) = &root {
        ctx.build(root)?;
    }
    let bytes = std::fs::read(bin).map_err(|_| format!("no binary at {}", bin.display()))?;
    Ok(LabelBin {
        path: text(bin),
        sha256: crate::committed::sha256_hex(&bytes),
        commit: root
            .as_ref()
            .map(|r| git(r, &["rev-parse", "HEAD"]).unwrap_or_default())
            .unwrap_or_default(),
        dirty: root.as_ref().is_some_and(|r| super::dirty(r)),
        built: root.is_some(),
    })
}

/// Why a binary may not score this label, or None when it may. A binary
/// from another commit, an unknown one, or a dirty tree would make the
/// label's `commit.txt` describe code that did not run.
pub fn bin_commit_refusal(bin: &LabelBin, label_commit: &str, force: bool) -> Option<String> {
    let why = if bin.commit.is_empty() {
        Some(
            "the binary's build commit is unknown (it is not in a cargo workspace's target/ dir)"
                .to_string(),
        )
    } else if bin.dirty {
        Some(format!(
            "the binary was built from a DIRTY tree at {} — it corresponds to no commit",
            short(&bin.commit)
        ))
    } else if !same_commit(&bin.commit, label_commit) {
        Some(format!(
            "the binary was built from {}, but this label records {}",
            short(&bin.commit),
            short(label_commit)
        ))
    } else {
        None
    };
    let why = why?;
    if force {
        return None;
    }
    Some(format!(
        "{why}.\nThe label's commit.txt would then describe code that did not produce its numbers.\n\
         Build from the label's commit, or pass --force-mixed if mixing is deliberate."
    ))
}

fn short(sha: &str) -> String {
    sha.chars().take(12).collect()
}

/// `build-bin <ref>`: build the pipeline binary of any commit of the repo
/// at `repo`, in a detached worktree under `builds` (kept: a rebuild of
/// the same commit is incremental). Refused for a ref from before the
/// cutover, which has no pipeline binary.
pub fn build_ref(
    ctx: &Ctx,
    repo: &Path,
    refname: &str,
    builds: &Path,
) -> Result<bin_build::BinRecord, String> {
    let sha = git(repo, &["rev-parse", &format!("{refname}^{{commit}}")])
        .map_err(|e| format!("{refname} is not a commit: {e}"))?;
    bin_build::pipeline_crate_at_commit(repo, &sha)?;
    let root = builds.join(short(&sha));
    if !root.exists() {
        std::fs::create_dir_all(builds).map_err(|e| format!("{}: {e}", builds.display()))?;
        git(repo, &["worktree", "add", "--detach", &text(&root), &sha])?;
        link_node_modules(repo, &root);
    }
    let rec = ctx.build(&root)?;
    if !same_commit(&rec.commit, &sha) {
        return Err(format!(
            "{} is at {}, not {sha}: the build worktree moved",
            root.display(),
            rec.commit
        ));
    }
    Ok(rec)
}

/// The binary shells out to the webcrack shim (webpack/browserify inputs),
/// which needs the repo's installed deps: link them into a worktree.
pub fn link_node_modules(repo: &Path, worktree: &Path) {
    let from = repo.join("node_modules");
    let to = worktree.join("node_modules");
    if from.exists() && !to.exists() {
        let _ = std::os::unix::fs::symlink(from, to);
    }
}

#[cfg(test)]
#[path = "bin_test.rs"]
mod bin_test;
