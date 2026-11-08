//! `index build <spec>`: build a library fingerprint index from its
//! committed spec, chunked (the port of `scripts/build-index.sh`, branch
//! exp184-index-spec, on today's binary: the builder is
//! `humanify-ag index build` since exp186).
//!
//! The builder holds the plan and the execution, but one builder process
//! runs at most `--jobs` package builds at a time, and a full build is
//! hours of installs and bundles. So: the dry run first (it fetches the
//! registry snapshot every chunk then reads offline; npm rate-limits a
//! bulk sweep, so it is retried, each attempt resuming), then N chunks AT
//! ONCE (disjoint packages; each version installs into its own work dir
//! and a shard is written then renamed, so concurrent chunks are safe),
//! then ONE `--merge-only`. A rerun skips every shard the store holds, so
//! a stopped build resumes.
//!
//! No host-wide lock (exp205): the coordinating agent decides what runs
//! beside a build, from its footprint. Two builds must still never write
//! one store (a shard's temp name is fixed per shard, so two writers of one
//! shard collide) or one work dir (logs, the dry run's state), so the build
//! CLAIMS both (`super::claim`): a second build on either is refused at
//! once, never queued. The registry snapshot is shared on purpose (a
//! fetched packument is never fetched again; the chunks read it offline).

use std::path::{Path, PathBuf};
use std::time::Duration;

use super::{Ctx, text};

/// A chunk's or the merge's whole run (hours of installs on a full spec).
const BUILD_DEADLINE: Duration = Duration::from_secs(48 * 3600);
/// The dry run (the registry fetch) and its retry backoff.
const DRY_RUN_DEADLINE: Duration = Duration::from_secs(6 * 3600);
const RETRY_BACKOFF: Duration = Duration::from_secs(300);
const MAX_DRY_RUN_ATTEMPTS: u32 = 24;

/// One build's settings.
#[derive(Clone, Debug)]
pub struct IndexBuild {
    pub spec: PathBuf,
    pub registry: PathBuf,
    pub store: PathBuf,
    pub work: PathBuf,
    pub out: PathBuf,
    pub chunks: u32,
    pub jobs: u32,
    pub merge_jobs: u32,
}

impl IndexBuild {
    fn common(&self) -> Vec<String> {
        vec![
            "index".into(),
            "build".into(),
            "--spec".into(),
            text(&self.spec),
            "--registry".into(),
            text(&self.registry),
            "--store".into(),
            text(&self.store),
            "--work".into(),
            text(&self.work),
        ]
    }
}

/// The dry run, retried until the registry fetch completes (each attempt
/// resumes; fetched packuments are never re-fetched).
fn dry_run(ctx: &Ctx, bin: &Path, b: &IndexBuild) -> Result<(), String> {
    let mut args = b.common();
    args.push("--dry-run".into());
    for attempt in 1..=MAX_DRY_RUN_ATTEMPTS {
        let log = b.work.join("dry-run.log");
        let s = ctx.spawn(&text(bin), &args, DRY_RUN_DEADLINE);
        let done = crate::proc::run_to_file(&s, &log, Duration::from_secs(3600), &mut |_| {});
        if matches!(done, Ok(ref e) if e.code == 0) {
            return Ok(());
        }
        ctx.say(format!(
            "registry fetch incomplete (attempt {attempt}/{MAX_DRY_RUN_ATTEMPTS}, see {}); retrying in 5 minutes",
            text(&log)
        ));
        backoff(RETRY_BACKOFF);
    }
    Err("the registry fetch never completed".into())
}

/// The retry envelope's backoff between dry-run attempts.
fn backoff(d: Duration) {
    // concurrency: the dry run's retry backoff (npm rate-limits a bulk
    // view sweep); an event wait on a channel nothing sends to, bounded.
    let (_tx, rx) = std::sync::mpsc::sync_channel::<()>(1);
    let _ = rx.recv_timeout(d);
}

/// Every chunk at once; Err names how many failed.
fn chunks(ctx: &Ctx, bin: &Path, b: &IndexBuild) -> Result<(), String> {
    let ids: Vec<u32> = (0..b.chunks).collect();
    let ok = super::each(&ids, ids.len(), |&i| {
        let mut args = b.common();
        args.extend([
            "--jobs".to_string(),
            b.jobs.to_string(),
            "--chunk".into(),
            format!("{i}/{}", b.chunks),
            "--offline".into(),
        ]);
        let log = b.work.join(format!("chunk-{i}-of-{}.log", b.chunks));
        let s = ctx.spawn(&text(bin), &args, BUILD_DEADLINE);
        matches!(
            crate::proc::run_to_file(&s, &log, Duration::from_secs(3600), &mut |_| {}),
            Ok(ref e) if e.code == 0
        )
    });
    let failed: Vec<u32> = ids
        .iter()
        .zip(&ok)
        .filter(|(_, ok)| !**ok)
        .map(|(i, _)| *i)
        .collect();
    if failed.is_empty() {
        return Ok(());
    }
    Err(format!(
        "{} of {} chunks failed ({}); not merging (a rerun resumes). Logs: {}/chunk-<i>-of-{}.log",
        failed.len(),
        b.chunks,
        failed
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(", "),
        text(&b.work),
        b.chunks
    ))
}

fn merge(ctx: &Ctx, bin: &Path, b: &IndexBuild) -> Result<(), String> {
    let mut args = b.common();
    args.extend([
        "--merge-jobs".to_string(),
        b.merge_jobs.to_string(),
        "--merge-only".into(),
        "--out".into(),
        text(&b.out),
    ]);
    let log = b.work.join("merge.log");
    let s = ctx.spawn(&text(bin), &args, BUILD_DEADLINE);
    match crate::proc::run_to_file(&s, &log, Duration::from_secs(3600), &mut |_| {}) {
        Ok(e) if e.code == 0 => Ok(()),
        Ok(e) => Err(format!("the merge exited {} (see {})", e.code, text(&log))),
        Err(e) => Err(e),
    }
}

/// `index build`: exit 0 when the index is written.
pub fn run(ctx: &Ctx, b: &IndexBuild) -> u8 {
    match run_inner(ctx, b) {
        Ok(()) => {
            ctx.say(format!("index written to {}", text(&b.out)));
            0
        }
        Err(e) => {
            ctx.say(e);
            1
        }
    }
}

fn run_inner(ctx: &Ctx, b: &IndexBuild) -> Result<(), String> {
    let what = format!("index build {}", text(&b.spec));
    let _store = super::claim::claim_folder(&b.store, &what)?;
    let _work = super::claim::claim_folder(&b.work, &what)?;
    let rec = ctx.build(&ctx.repo)?;
    for d in [&b.registry, &b.work] {
        std::fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
    }
    dry_run(ctx, &rec.path, b)?;
    chunks(ctx, &rec.path, b)?;
    merge(ctx, &rec.path, b)
}
