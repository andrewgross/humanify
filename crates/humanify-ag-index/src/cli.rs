//! `humanify-ag index`: the offline library fingerprint index builder
//! (docs/design/library-index.md), a subcommand of the one `humanify-ag`
//! binary (exp186; it was the separate `humanify-index` binary before).
//! Not part of a deobfuscation run: it reads the npm registry, installs
//! and bundles packages, and writes an index directory a run can later be
//! pointed at. The binary's global allocator (mimalloc, exp111) is the
//! builder's: glibc's per-thread arenas keep the peak of every large
//! bundle a worker fingerprinted, so a long build ratchets up to the worst
//! concurrent peak it ever met and stays there.
//!
//! Since exp184 the build is driven by ONE committed spec file
//! (`experiments/184-index-spec/README.md`, Andrew 2026-11-02: "it
//! shouldn't be 'here is a list of 25 random packages to also add and if
//! you don't have this hardcoded list then it won't work'"): `--spec
//! <file>` says which packages (rules over replayable public signals),
//! which versions of each (rules, with per-package overrides), the
//! recipes, the pinned tools and the refusals. The resolved plan — every
//! package@version and the rule that chose it — is written into the
//! index's `build.json` sidecar.

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::build::{BuildConfig, SpongeRegions, run};
use crate::cache::{BUILD_SIDECAR_SCHEMA, BuildSidecar, SpecSidecar};
use crate::planner::{self, ResolvedSpec};
use crate::spec::{IndexSpec, SPEC_SCHEMA};
use crate::tools;

/// `humanify-ag index <command>`: build a library fingerprint index from a
/// spec, or slim a work dir's build maps.
#[derive(clap::Subcommand)]
pub enum IndexCommand {
    /// Build the index an index spec describes: resolve the spec's
    /// package and version rules against the npm registry, install and
    /// bundle what is missing, merge into an index, record the resolved
    /// plan in its `build.json` sidecar.
    Build {
        /// The index spec (schema `humanify-index-spec/1`): the as-of
        /// date, the package selections (npm-high-impact lists at pinned
        /// versions + the dependency closure of their chosen versions),
        /// the version rules and overrides, the recipes, the pinned
        /// tools, the refusals.
        #[arg(long, required_unless_present = "replay", conflicts_with = "replay")]
        spec: Option<PathBuf>,
        /// Instead of a spec: rebuild an EXISTING index's plan — its
        /// manifest's package@versions, as-of, recipes and bundler pins —
        /// under this binary's fingerprint forms (exp204: a form era
        /// re-keys every module shard while the stored installs and
        /// bundles stay valid, so only the fingerprinting and the merge
        /// rerun). Not a way to choose packages: that is the spec's.
        #[arg(long, requires = "sponge_regions")]
        replay: Option<PathBuf>,
        /// With --replay: what a pre-bundled file's foreign regions become
        /// (`drop`, `origin`) — the source index's setting, which its
        /// manifest does not record.
        #[arg(long, requires = "replay")]
        sponge_regions: Option<String>,
        /// The registry snapshot directory (packuments, enriched with
        /// per-version dependency metadata; fetched once).
        #[arg(long)]
        registry: PathBuf,
        /// The shard store (reused across builds and dates); also where
        /// the spec's pinned tools are installed (`<store>/tools/`).
        #[arg(long)]
        store: PathBuf,
        /// Where packages are installed and bundled, and where the spec's
        /// list files are cached (`<work>/lists/`).
        #[arg(long)]
        work: PathBuf,
        /// The index directory to write (not needed with --dry-run or
        /// --chunk).
        #[arg(long)]
        out: Option<PathBuf>,
        /// Parallel package builds (at most 4).
        #[arg(long, default_value_t = 2)]
        jobs: usize,
        /// Parallel merge workers (exp162): the merge partitions the
        /// unit-key space into N contiguous ranges, one worker each; every
        /// worker streams all shards in label order and keeps its range's
        /// rows, and the segments concatenate in range order — the same
        /// bytes the serial merge writes, which is the default (1).
        #[arg(long, default_value_t = 1)]
        merge_jobs: usize,
        /// Never fetch: every packument must already be in --registry.
        #[arg(long)]
        offline: bool,
        /// Build no shard: merge the shards the store already holds (a
        /// version with none is refused `no-shard`).
        #[arg(long)]
        merge_only: bool,
        /// Build only chunk I of N: the plan is resolved in FULL (the
        /// same everywhere), the builds and the merge are limited to the
        /// chunk's packages — a heavy build split across invocations,
        /// finished by a final `--chunk 0/1` or a `--merge-only`. Nothing
        /// is written to --out.
        #[arg(long)]
        chunk: Option<String>,
        /// Print the resolved plan (every package@version and the rule
        /// that chose it, the refusals and the skips) and stop.
        #[arg(long)]
        dry_run: bool,
    },
    /// Slim the builds a work dir already holds (exp131): every build map
    /// loses its `sourcesContent` (the builder never reads it), rewritten
    /// in place through its inode, so hard links across work dirs stay one
    /// shared file. Symlinks and `node_modules` are never touched. Parallel
    /// over the process's CPUs: bound it with `taskset`. Nothing may read
    /// the work dirs meanwhile (take the build lock).
    SlimCache {
        /// A work dir (repeatable): its `<install>/build/**/*.map`.
        #[arg(long = "work", required = true)]
        work: Vec<PathBuf>,
        /// Where each rewrite is journaled first (a rerun finishes an
        /// interrupted one).
        #[arg(long)]
        journal: PathBuf,
    },
}

/// `slim-cache`: progress on stderr, the report as JSON on stdout; an
/// error on any map fails the run (the maps it did slim stay slimmed).
fn slim_cache(work: &[PathBuf], journal: &std::path::Path) -> Result<(), String> {
    let line = |r: &crate::slim::SlimReport| {
        serde_json::json!({
            "maps": r.maps, "stripped": r.stripped, "absent": r.absent,
            "bytes_before": r.bytes_before, "bytes_after": r.bytes_after,
            "skipped_symlinks": r.skipped_symlinks, "recovered": r.recovered,
            "unreadable": r.unreadable.len(), "errors": r.errors.len(),
        })
    };
    let report = crate::slim::slim_cache(work, journal, &mut |r| eprintln!("{}", line(r)))?;
    for (p, e) in &report.unreadable {
        eprintln!("UNREADABLE {}: {e}", p.display());
    }
    for e in &report.errors {
        eprintln!("ERROR {e}");
    }
    println!("{}", line(&report));
    if report.errors.is_empty() {
        Ok(())
    } else {
        Err(format!("{} maps failed", report.errors.len()))
    }
}

/// The spec's list files, fetched and hash-verified into
/// `<work>/lists/` (cached: a rerun replays them).
fn list_fetcher(
    work: &std::path::Path,
) -> impl Fn(crate::spec::HighImpactList, &str, &str) -> Result<String, String> {
    let dir = work.join("lists");
    move |list, version, sha256| planner::fetch_high_impact_list(list, version, sha256, &dir)
}

/// The resolved plan, as `--dry-run` prints it: every package@version and
/// the rule that chose it.
fn print_plan(spec: &IndexSpec, resolved: &ResolvedSpec) {
    println!("spec as-of {}", spec.as_of);
    for (id, names) in &resolved.skipped {
        for n in names {
            println!("skipped {n}: refusal {id}");
        }
    }
    for s in &resolved.selections {
        match s.attribution.as_deref() {
            Some(a) => println!("selected {} ({a})", s.name),
            None => println!("selected {}", s.name),
        }
        for v in &s.versions {
            match v.attribution.as_deref() {
                Some(a) => println!("  index {}@{} ({}, {a})", s.name, v.version, v.role),
                None => println!("  index {}@{} ({})", s.name, v.version, v.role),
            }
        }
        for (version, reason) in &s.refused {
            println!("  refused {}@{}: {reason}", s.name, version);
        }
    }
}

/// The `build` subcommand's paths and counts. Every path the bundlers or
/// npm touch must be absolute.
struct BuildArgs {
    registry: PathBuf,
    store: PathBuf,
    work: PathBuf,
    out: Option<PathBuf>,
    jobs: usize,
    merge_jobs: usize,
    offline: bool,
    merge_only: bool,
    chunk: Option<String>,
    dry_run: bool,
}

/// A chunk spec `I/N`: 0 <= I < N.
fn parse_chunk(text: &str) -> Result<(usize, usize), String> {
    let Some((i, n)) = text.split_once('/') else {
        return Err(format!("--chunk {text}: expected I/N"));
    };
    let (i, n) = (i.trim(), n.trim());
    let (i, n) = (i.parse::<usize>(), n.parse::<usize>());
    let (Ok(i), Ok(n)) = (i, n) else {
        return Err(format!("--chunk {text}: expected I/N (numbers)"));
    };
    if n == 0 || i >= n {
        return Err(format!("--chunk {text}: need 0 <= I < N"));
    }
    Ok((i, n))
}

/// The index spec resolved against the registry: the spec's parse (whose
/// sha256 goes into the sidecar), the planner's resolution, and the
/// spec's pinned tools fetched into the store.
fn resolve_spec(
    spec_path: &std::path::Path,
    registry: &std::path::Path,
    store: &std::path::Path,
    work: &std::path::Path,
    offline: bool,
) -> Result<(IndexSpec, ResolvedSpec, String), String> {
    let text =
        std::fs::read_to_string(spec_path).map_err(|e| format!("{}: {e}", spec_path.display()))?;
    let sha = crate::shard::hex_sha256(text.as_bytes());
    let spec = IndexSpec::parse(&text)?;
    let mut loader = planner::SnapshotLoader::new(registry.to_path_buf(), offline, 16);
    let mut src = planner::Sources {
        loader: &mut loader,
        list: &list_fetcher(work),
    };
    let resolved = planner::resolve(&spec, &mut src)?;
    for (bundler, version) in &spec.tools {
        tools::tool_at(store, *bundler, version).map_err(|e| format!("spec tools: {e}"))?;
    }
    Ok((spec, resolved, sha))
}

/// The sidecar spec block: the resolved plan, rule attribution included.
fn spec_sidecar(spec: &IndexSpec, resolved: &ResolvedSpec, spec_sha: &str) -> SpecSidecar {
    SpecSidecar {
        spec_sha256: spec_sha.to_string(),
        schema: SPEC_SCHEMA.into(),
        as_of: spec.as_of.clone(),
        default_policy: spec.versions.default.name(),
        dependents: spec.versions.dependents,
        tools: spec
            .tools
            .iter()
            .map(|(b, v)| (b.name().to_string(), v.clone()))
            .collect(),
        selections: resolved
            .selections
            .iter()
            .map(|s| crate::cache::SpecSelection {
                name: s.name.clone(),
                attribution: s.attribution.clone().unwrap_or_default(),
                versions: s
                    .versions
                    .iter()
                    .map(|v| crate::cache::SpecPlannedVersion {
                        version: v.version.clone(),
                        role: v.role.clone(),
                        published: v.published.clone(),
                        attribution: v.attribution.clone().unwrap_or_default(),
                    })
                    .collect(),
                refused: s.refused.clone(),
            })
            .collect(),
        skipped: resolved.skipped.clone(),
    }
}

/// What made an index, for its sidecar.
struct Made {
    built_from: String,
    as_of: String,
    packages: usize,
}

/// The finished index's `build.json` sidecar: the outcome's counts, what
/// made it, and (a spec build) the resolved plan.
fn write_sidecar(
    out: &std::path::Path,
    outcome: &crate::build::BuildOutcome,
    made: Made,
    spec: Option<SpecSidecar>,
) -> Result<(), String> {
    let side = BuildSidecar {
        schema: BUILD_SIDECAR_SCHEMA.into(),
        index_id: outcome.index_id.clone(),
        built_from: made.built_from,
        as_of: made.as_of,
        format: humanify_ag_core::libindex::format::INDEX_FORMAT.into(),
        slices: outcome
            .slices
            .iter()
            .map(|s| crate::cache::SidecarSlice {
                slice: s.slice.clone(),
                recipe: s.recipe.clone(),
                bundler: s.bundler.clone(),
                bundler_version: s.version.clone(),
            })
            .collect(),
        seeds: Vec::new(),
        dropped: Vec::new(),
        failed: outcome
            .errors
            .iter()
            .map(|(label, why)| crate::cache::DroppedSeed {
                name: label.clone(),
                reason: why.clone(),
                evidence: String::new(),
            })
            .collect(),
        versions_in_index: outcome.tasks - outcome.errors.len(),
        packages: made.packages,
        refused: outcome.refused.len(),
        built: outcome.built,
        reused: outcome.reused,
        stage_ms: BTreeMap::new(),
        completed: crate::cache::now_iso(),
        spec,
    };
    crate::cache::write_build_sidecar(out, &side)
}

fn build(spec_path: &std::path::Path, args: &BuildArgs) -> Result<String, String> {
    let (spec, resolved, spec_sha) = resolve_spec(
        spec_path,
        &args.registry,
        &args.store,
        &args.work,
        args.offline,
    )?;
    if args.dry_run {
        print_plan(&spec, &resolved);
        return Ok(String::new());
    }
    let chunk = args.chunk.as_deref().map(parse_chunk).transpose()?;
    // every recipe's bundler tool, at the spec's pin
    let mut builds: Vec<crate::recipe::RecipeBuild> = Vec::new();
    for recipe in &spec.recipes {
        let version = spec.tools.get(&recipe.bundler).expect("checked at parse");
        let tool = tools::tool_at(&args.store, recipe.bundler, version)?;
        builds.push(crate::recipe::RecipeBuild {
            recipe: *recipe,
            bin: tool.bin.clone(),
        });
    }
    let side_spec = spec_sidecar(&spec, &resolved, &spec_sha);
    let cfg = BuildConfig {
        packages: Vec::new(),
        spec: Some(resolved),
        as_of: spec.as_of.clone(),
        registry: args.registry.clone(),
        store: args.store.clone(),
        work: args.work.clone(),
        out: args.out.clone(),
        recipes: builds,
        jobs: args.jobs,
        merge_jobs: args.merge_jobs,
        offline: args.offline,
        merge_only: args.merge_only,
        refuse: Vec::new(),
        quiet: false,
        names: spec.names,
        sponge_regions: spec.sponge_regions,
        pinned: Vec::new(),
        policy: crate::asof::VersionPolicy::AsOf,
        chunk,
    };
    let outcome = run(&cfg, false)?;
    if let (Some(out), false) = (&args.out, chunk.is_some()) {
        let packages = side_spec.selections.len();
        let made = Made {
            built_from: "index spec".into(),
            as_of: spec.as_of.clone(),
            packages,
        };
        write_sidecar(out, &outcome, made, Some(side_spec))?;
    }
    Ok(outcome.index_id)
}

/// `--replay <index-dir>` (exp204, `crate::replay`): the source index's
/// exact plan, built under this binary's fingerprint forms. The pins go
/// through the pinned-plan path the run-time build uses (`BuildConfig::pinned`,
/// policy `PinsOnly`), so a version is re-planned against the same registry
/// snapshot and indexed iff it still resolves — its stored install and
/// bundles are reused when their stamps match.
fn build_replay(
    index: &std::path::Path,
    sponge_regions: SpongeRegions,
    args: &BuildArgs,
) -> Result<String, String> {
    let manifest = index.join("manifest.json");
    let text =
        std::fs::read_to_string(&manifest).map_err(|e| format!("{}: {e}", manifest.display()))?;
    let replay = crate::replay::replay_plan(&text)?;
    let chunk = args.chunk.as_deref().map(parse_chunk).transpose()?;
    let mut builds: Vec<crate::recipe::RecipeBuild> = Vec::new();
    for (recipe, version) in &replay.recipes {
        let tool = tools::tool_at(&args.store, recipe.bundler, version)?;
        builds.push(crate::recipe::RecipeBuild {
            recipe: *recipe,
            bin: tool.bin.clone(),
        });
    }
    let cfg = BuildConfig {
        packages: Vec::new(),
        spec: None,
        as_of: replay.as_of.clone(),
        registry: args.registry.clone(),
        store: args.store.clone(),
        work: args.work.clone(),
        out: args.out.clone(),
        recipes: builds,
        jobs: args.jobs,
        merge_jobs: args.merge_jobs,
        offline: args.offline,
        merge_only: args.merge_only,
        refuse: Vec::new(),
        quiet: false,
        names: replay.names,
        sponge_regions,
        pinned: replay.pinned.clone(),
        policy: crate::asof::VersionPolicy::PinsOnly,
        chunk,
    };
    if args.dry_run {
        run(&cfg, true)?;
        return Ok(String::new());
    }
    let outcome = run(&cfg, false)?;
    if let (Some(out), false) = (&args.out, chunk.is_some()) {
        let packages: std::collections::BTreeSet<&str> = replay
            .pinned
            .iter()
            .map(|p| p.rsplit_once('@').map_or(p.as_str(), |(n, _)| n))
            .collect();
        let made = Made {
            built_from: format!("replay of index {}", index.display()),
            as_of: replay.as_of.clone(),
            packages: packages.len(),
        };
        write_sidecar(out, &outcome, made, None)?;
    }
    Ok(outcome.index_id)
}

/// Run one `humanify-ag index` command: its output on stdout, progress
/// and errors on stderr, and the process exit code (0, or 1 on an error).
pub fn run_index(command: IndexCommand) -> i32 {
    if let IndexCommand::SlimCache { work, journal } = &command {
        if let Err(e) = slim_cache(work, journal) {
            eprintln!("ERROR: {e}");
            return 1;
        }
        return 0;
    }
    let IndexCommand::Build {
        spec,
        replay,
        sponge_regions,
        registry,
        store,
        work,
        out,
        jobs,
        merge_jobs,
        offline,
        merge_only,
        chunk,
        dry_run,
    } = command
    else {
        unreachable!("slim-cache returned above")
    };
    let result = (|| -> Result<String, String> {
        if out.is_none() && !dry_run && chunk.is_none() {
            return Err("--out is required unless --dry-run or --chunk".into());
        }
        // the bundlers and npm run inside each install: every path absolute
        let abs = |p: PathBuf| -> Result<PathBuf, String> {
            std::path::absolute(&p).map_err(|e| format!("{}: {e}", p.display()))
        };
        let args = BuildArgs {
            registry: abs(registry)?,
            store: abs(store)?,
            work: abs(work)?,
            out: out.map(abs).transpose()?,
            jobs,
            merge_jobs,
            offline,
            merge_only,
            chunk,
            dry_run,
        };
        match (spec, replay) {
            (Some(spec), None) => build(&spec, &args),
            (None, Some(index)) => {
                let regions = sponge_regions.as_deref().unwrap_or_default();
                let regions = SpongeRegions::parse(regions)
                    .ok_or_else(|| format!("--sponge-regions {regions:?} (drop, origin)"))?;
                build_replay(&abs(index)?, regions, &args)
            }
            _ => Err("exactly one of --spec and --replay".into()),
        }
    })();
    match result {
        Ok(id) if !id.is_empty() => {
            println!("index id {id}");
            0
        }
        Ok(_) => 0,
        Err(e) => {
            eprintln!("ERROR: {e}");
            1
        }
    }
}
