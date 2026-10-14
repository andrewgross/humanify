//! exp103 synthetic scaler: `exp103-scale <index /2 dir> <v3 dir> <slice> <k> <out dir>`.
//!
//! A k-times larger /3 index for COST measurement only (its claims mean
//! nothing): k copies of every unit row of `<slice>` and every literal
//! file. Copy c > 0 renames each owner `<owner>~c`, moves every unit hash
//! to another key (XOR a per-copy constant), and keeps half the literals
//! (those whose key is even) shared with copy 0 while the other half become
//! copy-specific strings (`<lit>~c`): the distinct-literal count grows
//! (k+1)/2 times and the shared literals' postings k times. Written with
//! humanify-core's own encoder; the manifest is the v3 one (every slice
//! listed; only `<slice>`'s unit table is written: run it on bundles that
//! read that slice).

use std::collections::BTreeMap;
use std::path::Path;

use humanify_core::libindex::format::{LiteralRow, UnitRow};
use humanify_core::libindex::table::{literal_key, lookup_files};

fn rows<T: serde::de::DeserializeOwned>(path: &Path) -> Vec<T> {
    std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
        .lines()
        .map(|l| serde_json::from_str(l).expect("a row"))
        .collect()
}

fn mix(c: u64) -> u64 {
    let mut z = c.wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (v2, v3, slice, k, out) = (
        Path::new(&a[1]),
        Path::new(&a[2]),
        a[3].as_str(),
        a[4].parse::<u64>().expect("k"),
        Path::new(&a[5]),
    );
    assert!(!out.exists(), "{} exists", out.display());
    let base_units: Vec<UnitRow> = rows(&v3.join(format!("units.{slice}.jsonl")));
    let base_lits: Vec<LiteralRow> = rows(&v2.join("literals.jsonl"));
    let mut units = Vec::with_capacity(base_units.len() * k as usize);
    let mut literals = Vec::with_capacity(base_lits.len() * k as usize);
    for c in 0..k {
        let owner = |o: &str| {
            if c == 0 {
                o.to_string()
            } else {
                format!("{o}~{c}")
            }
        };
        let salt = if c == 0 { 0 } else { mix(c) };
        for r in &base_units {
            let h = u64::from_str_radix(&r.hash, 16).expect("hex") ^ salt;
            units.push(UnitRow {
                hash: format!("{h:016x}"),
                owner: owner(&r.owner),
                ..r.clone()
            });
        }
        for r in &base_lits {
            let lits = r
                .lits
                .iter()
                .map(|l| {
                    if c == 0 || literal_key(l) % 2 == 0 {
                        l.clone()
                    } else {
                        format!("{l}~{c}")
                    }
                })
                .collect();
            literals.push(LiteralRow {
                owner: owner(&r.owner),
                file: r.file.clone(),
                lits,
                versions: r.versions.clone(),
            });
        }
    }
    drop(base_lits);
    let by_slice: BTreeMap<String, Vec<UnitRow>> =
        [(slice.to_string(), units)].into_iter().collect();
    let files = lookup_files(&by_slice, &literals).expect("encodes");
    std::fs::create_dir_all(out).expect("mkdir");
    for (name, bytes) in &files {
        std::fs::write(out.join(name), bytes).expect("write");
    }
    std::fs::copy(v3.join("manifest.json"), out.join("manifest.json")).expect("manifest");
    let total: usize = files.values().map(Vec::len).sum();
    println!(
        "k {k}: unit rows {}, literal files {}, lookup bytes {total}",
        by_slice[slice].len(),
        literals.len()
    );
}
