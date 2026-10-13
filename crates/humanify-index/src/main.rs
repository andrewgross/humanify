//! `humanify-index`: the offline library fingerprint index builder
//! (docs/design/library-index.md). Not part of a humanify run: it reads
//! the npm registry, installs and bundles packages, and writes an index
//! directory a run can later be pointed at.

use std::path::PathBuf;

use clap::Parser;
use humanify_core::libindex::IndexSlice;
use humanify_index::asof::{VersionPolicy, parse_as_of};
use humanify_index::build::{BuildConfig, run};
use humanify_index::recipe::Recipe;

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
        /// Bundler slices to build: `bun`, `esbuild` (comma-separated).
        #[arg(long, default_value = "bun,esbuild")]
        slices: String,
        /// The Bun binary (the bun slice).
        #[arg(long, default_value = "bun")]
        bun: PathBuf,
        /// The esbuild binary (the esbuild slice).
        #[arg(long, default_value = "esbuild")]
        esbuild: PathBuf,
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
        /// Print the plan (versions, refusals, shards to build or reuse)
        /// and stop.
        #[arg(long)]
        dry_run: bool,
    },
}

fn main() {
    let Command::Build {
        packages,
        pinned,
        as_of,
        registry,
        store,
        work,
        out,
        slices,
        bun,
        esbuild,
        jobs,
        policy,
        offline,
        dry_run,
    } = Cli::parse().command;
    let result = (|| -> Result<String, String> {
        let as_of = parse_as_of(&as_of)?;
        let specs = std::fs::read_to_string(&packages)
            .map_err(|e| format!("{}: {e}", packages.display()))?;
        let mut recipes = Vec::new();
        for name in slices.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            let slice = IndexSlice::from_name(name).ok_or(format!("unknown slice {name}"))?;
            let bin = match slice {
                IndexSlice::Bun => bun.clone(),
                IndexSlice::Esbuild => esbuild.clone(),
            };
            recipes.push(Recipe { slice, bin });
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
            slices: recipes,
            jobs,
            offline,
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
