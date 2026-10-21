//! `humanify-index`: the offline library fingerprint index builder
//! (docs/design/library-index.md). Not part of a humanify run: it reads
//! the npm registry, installs and bundles packages, and writes an index
//! directory a run can later be pointed at.

/// mimalloc, not glibc malloc (exp111): glibc's per-thread arenas keep
/// the peak of every large bundle a worker fingerprinted, so a long build
/// ratchets up to the worst concurrent peak it ever met and stays there.
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

use std::path::PathBuf;

use clap::Parser;
use humanify_core::libindex::recipe::{Recipe, RecipeBundler};
use humanify_index::asof::{VersionPolicy, parse_as_of};
use humanify_index::build::{BuildConfig, SpongeRegions, run};
use humanify_index::recipe::RecipeBuild;

#[derive(Parser)]
#[command(
    name = "humanify-index",
    about = "Build a library fingerprint index as of a date"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(clap::Subcommand)]
#[expect(
    clippy::large_enum_variant,
    reason = "parsed once per process; boxing clap's arguments buys nothing"
)]
enum Command {
    /// Plan, build the missing shards, merge them into an index.
    Build {
        /// A file of package specs, one per line: `name` or `name@version`
        /// (a pinned version, refused if published after --as-of).
        #[arg(long)]
        packages: PathBuf,
        /// A file of `name@version` specs indexed at exactly those versions
        /// (no policy picks): the versions a bundle's own readers declare.
        #[arg(long)]
        pinned: Option<PathBuf>,
        /// Index only versions published on or before this time
        /// (`YYYY-MM-DD` = the end of that UTC day, or a UTC timestamp).
        #[arg(long)]
        as_of: String,
        /// The registry snapshot directory (packuments; fetched once).
        #[arg(long)]
        registry: PathBuf,
        /// The shard store (reused across builds and dates).
        #[arg(long)]
        store: PathBuf,
        /// Where packages are installed and bundled.
        #[arg(long)]
        work: PathBuf,
        /// The index directory to write (not needed with --dry-run).
        #[arg(long)]
        out: Option<PathBuf>,
        /// A build recipe = one index slice (repeatable, at least one; no
        /// default):
        /// `<bun|esbuild>:target=<bun|node>,format=<cjs|esm>[,minify][,production]`
        /// (`production`: process.env.NODE_ENV defined as "production", as
        /// app bundles are built; Bun and esbuild only), e.g.
        /// `bun:target=bun,format=cjs,minify`. The first recipe of a
        /// bundler is its default slice (for bundles that do not record
        /// their target).
        #[arg(long = "recipe", required = true)]
        recipes: Vec<String>,
        /// The Bun binary (Bun recipes).
        #[arg(long, default_value = "bun")]
        bun: PathBuf,
        /// The esbuild binary (esbuild recipes).
        #[arg(long, default_value = "esbuild")]
        esbuild: PathBuf,
        /// The webpack recipe script (webpack recipes): a copy of
        /// `crates/humanify-index/recipes/webpack-build.cjs` beside an
        /// install of webpack, run with `node`.
        #[arg(long)]
        webpack: Option<PathBuf>,
        /// The rollup recipe script (rollup recipes): a copy of
        /// `crates/humanify-index/recipes/rollup-build.mjs` beside an
        /// install of rollup and its plugins, run with `node`.
        #[arg(long)]
        rollup: Option<PathBuf>,
        /// Parallel package builds (at most 4).
        #[arg(long, default_value_t = 2)]
        jobs: usize,
        /// Which versions to index: `as-of` (latest, previous patch, the
        /// two previous majors) or `year-all+minor` (every release of the
        /// last 12 months + the latest patch of every older minor).
        #[arg(long, default_value = "as-of")]
        policy: String,
        /// Never fetch: every packument must already be in --registry.
        #[arg(long)]
        offline: bool,
        /// Build no shard: merge the shards the store already holds (a
        /// version with none is refused `no-shard`). A re-merge after a
        /// build, without retrying the builds that failed.
        #[arg(long)]
        merge_only: bool,
        /// What the units inside a pre-bundled file's FOREIGN regions (a
        /// module marker naming another package) become: `drop` (no rows)
        /// or `origin` (rows of the package the marker names, at its pnpm
        /// path's version).
        #[arg(long, default_value = "drop")]
        sponge_regions: String,
        /// Print the plan (versions, refusals, shards to build or reuse)
        /// and stop.
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
    let line = |r: &humanify_index::slim::SlimReport| {
        serde_json::json!({
            "maps": r.maps, "stripped": r.stripped, "absent": r.absent,
            "bytes_before": r.bytes_before, "bytes_after": r.bytes_after,
            "skipped_symlinks": r.skipped_symlinks, "recovered": r.recovered,
            "unreadable": r.unreadable.len(), "errors": r.errors.len(),
        })
    };
    let report =
        humanify_index::slim::slim_cache(work, journal, &mut |r| eprintln!("{}", line(r)))?;
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

fn main() {
    let command = Cli::parse().command;
    if let Command::SlimCache { work, journal } = &command {
        if let Err(e) = slim_cache(work, journal) {
            eprintln!("ERROR: {e}");
            std::process::exit(1);
        }
        return;
    }
    let Command::Build {
        packages,
        pinned,
        as_of,
        registry,
        store,
        work,
        out,
        recipes,
        bun,
        esbuild,
        webpack,
        rollup,
        jobs,
        policy,
        offline,
        merge_only,
        sponge_regions,
        dry_run,
    } = command
    else {
        unreachable!("slim-cache returned above")
    };
    let result = (|| -> Result<String, String> {
        let as_of = parse_as_of(&as_of)?;
        let specs = std::fs::read_to_string(&packages)
            .map_err(|e| format!("{}: {e}", packages.display()))?;
        let mut builds: Vec<RecipeBuild> = Vec::new();
        for text in &recipes {
            let recipe = Recipe::parse(text)?;
            if builds.iter().any(|b| b.recipe == recipe) {
                return Err(format!("--recipe {text} given twice"));
            }
            // the scripts run inside each install: absolute, like every path
            let script = |given: &Option<PathBuf>, flag: &str| {
                let p = given
                    .clone()
                    .ok_or(format!("--recipe {text} needs --{flag} <recipe script>"))?;
                std::path::absolute(&p).map_err(|e| format!("{}: {e}", p.display()))
            };
            let bin = match recipe.bundler {
                RecipeBundler::Bun => bun.clone(),
                RecipeBundler::Esbuild => esbuild.clone(),
                RecipeBundler::Webpack => script(&webpack, "webpack")?,
                RecipeBundler::Rollup => script(&rollup, "rollup")?,
            };
            builds.push(RecipeBuild { recipe, bin });
        }
        if out.is_none() && !dry_run {
            return Err("--out is required unless --dry-run".into());
        }
        // the bundlers run inside each install: every path must be absolute
        let abs = |p: PathBuf| std::path::absolute(&p).map_err(|e| format!("{}: {e}", p.display()));
        let cfg = BuildConfig {
            packages: specs.lines().map(str::to_string).collect(),
            as_of,
            registry: abs(registry)?,
            store: abs(store)?,
            work: abs(work)?,
            out: out.map(abs).transpose()?,
            recipes: builds,
            jobs,
            offline,
            merge_only,
            sponge_regions: match sponge_regions.as_str() {
                "drop" => SpongeRegions::Drop,
                "origin" => SpongeRegions::Origin,
                other => return Err(format!("unknown --sponge-regions {other} (drop, origin)")),
            },
            pinned: match &pinned {
                Some(p) => std::fs::read_to_string(p)
                    .map_err(|e| format!("{}: {e}", p.display()))?
                    .lines()
                    .map(str::to_string)
                    .collect(),
                None => Vec::new(),
            },
            policy: VersionPolicy::from_name(&policy)
                .ok_or(format!("unknown --policy {policy} (as-of, year-all+minor)"))?,
        };
        run(&cfg, dry_run)
    })();
    match result {
        Ok(id) if !id.is_empty() => println!("index id {id}"),
        Ok(_) => {}
        Err(e) => {
            eprintln!("ERROR: {e}");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
#[path = "main_test.rs"]
mod main_test;
