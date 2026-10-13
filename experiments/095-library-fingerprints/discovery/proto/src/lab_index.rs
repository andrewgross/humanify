//! exp095 discovery lane: a library index with statement-level units, built
//! by the BUILDER's own code (`humanify_index::build::unit_rows`,
//! `literal_shard`, `merge`) from bundles and installs the lab already has
//! on disk, so no npm install or bundler run is repeated.
//!
//! Usage: exp095-lab-index <out-dir> <slice> <builds.txt> <pkgs.txt>
//!   builds.txt: one directory per line holding out.js + out.js.map
//!   pkgs.txt:   one install directory per line holding node_modules/
use std::path::{Path, PathBuf};

use humanify_core::libindex::IndexSlice;
use humanify_index::build::{literal_shard, unit_rows};
use humanify_index::merge::{MergeInput, merge};
use humanify_index::shard::{Shard, ShardInputs, ShardKind};
use humanify_index::sourcemap::SourceMap;
use rayon::prelude::*;

fn lines(path: &str) -> Vec<PathBuf> {
    std::fs::read_to_string(path)
        .expect("list")
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(PathBuf::from)
        .collect()
}

fn inputs(kind: ShardKind, label: &str) -> ShardInputs {
    ShardInputs {
        kind,
        package: label.to_string(),
        version: "lab".into(),
        bundler: None,
        recipe: "lab".into(),
        fingerprint: "lab".into(),
    }
}

fn unit_shard(slice: IndexSlice, dir: &Path) -> Option<Shard> {
    let code = std::fs::read_to_string(dir.join("out.js")).ok()?;
    let map = SourceMap::parse(&std::fs::read_to_string(dir.join("out.js.map")).ok()?).ok()?;
    let mut shard = Shard::new(&inputs(ShardKind::Modules(slice), &dir.to_string_lossy()));
    // The lab's Bun maps name their sources from one directory below the
    // build's (label.mjs's convention, `<dir>/out`); its esbuild maps from
    // the build's own directory, as the spec says. Take the base that
    // resolves.
    let base = map
        .sources
        .first()
        .map(|s| dir.join(s))
        .filter(|p| p.exists())
        .map_or_else(|| dir.join("out"), |_| dir.to_path_buf());
    let (units, notes) = unit_rows(slice, &code, &map, &base);
    shard.units = units;
    shard.notes.unit_mixed_spans = notes.mixed_spans;
    shard.notes.unowned = notes.unowned;
    if std::env::var("LAB_DEBUG").is_ok() {
        eprintln!(
            "{}: units {} mixed {} unowned {} unkeyed {}",
            dir.display(),
            shard.units.len(),
            notes.mixed_spans,
            notes.unowned,
            notes.unkeyed
        );
    }
    Some(shard)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let out = PathBuf::from(&args[1]);
    let slice = IndexSlice::from_name(&args[2]).expect("slice");
    let builds = lines(&args[3]);
    let pkgs = lines(&args[4]);
    let t0 = std::time::Instant::now();
    let mut shards: Vec<Shard> = builds
        .par_iter()
        .filter_map(|d| unit_shard(slice, d))
        .collect();
    let units: usize = shards.iter().map(|s| s.units.len()).sum();
    eprintln!(
        "{} builds -> {} unit rows in {:.0}s",
        shards.len(),
        units,
        t0.elapsed().as_secs_f64()
    );
    let t1 = std::time::Instant::now();
    let lits: Vec<Shard> = pkgs
        .par_iter()
        .map(|d| literal_shard(d, &inputs(ShardKind::Literals, &d.to_string_lossy()), &[]))
        .collect();
    eprintln!(
        "{} installs -> {} literal files in {:.0}s",
        lits.len(),
        lits.iter().map(|s| s.literals.len()).sum::<usize>(),
        t1.elapsed().as_secs_f64()
    );
    shards.extend(lits);
    let files = merge(&MergeInput {
        as_of: "lab".into(),
        shards,
        ..MergeInput::default()
    });
    std::fs::create_dir_all(&out).expect("out");
    for (name, bytes) in &files.files {
        std::fs::write(out.join(name), bytes).expect("write");
    }
    eprintln!("index {} -> {}", files.id, out.display());
}
