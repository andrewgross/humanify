//! usage: exp115-lab-mark <index /3 dir> <out dir>
//! Hard-links every file but the unit tables and the manifest; re-marks and re-encodes each slice's unit rows; writes
//! vendored.json and a /4 manifest (table hashes updated).
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use humanify_core::libindex::families::PackageFamilies;
use humanify_core::libindex::format::{INDEX_FORMAT, Manifest, UnitRow};
use humanify_core::libindex::table::UnitTable;
use humanify_core::libindex::vendored::mark_vendored;
use sha2::{Digest, Sha256};

fn hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (src, out) = (Path::new(&a[1]), Path::new(&a[2]));
    assert!(!out.exists(), "REFUSED: {} exists", out.display());
    std::fs::create_dir_all(out).unwrap();
    let mtext = std::fs::read_to_string(src.join("manifest.json")).unwrap();
    let mut manifest: Manifest = serde_json::from_str(&mtext).unwrap();
    let packages: Vec<String> =
        serde_json::from_str(&std::fs::read_to_string(src.join("packages.json")).unwrap()).unwrap();
    let names: BTreeSet<String> = packages.iter().cloned().collect();
    let families = PackageFamilies::shipped().with_shipped_links_among(&names);
    let known = |n: &str| names.contains(n);
    let same = |x: &str, y: &str| families.same(x, y);
    let mut slices = BTreeMap::new();
    for e in std::fs::read_dir(src).unwrap() {
        let name = e.unwrap().file_name().into_string().unwrap();
        if name == "manifest.json" || (name.starts_with("units.") && name.ends_with(".bin")) {
            continue;
        }
        if let Some(slice) = name
            .strip_prefix("units.")
            .and_then(|s| s.strip_suffix(".jsonl"))
        {
            let text = std::fs::read_to_string(src.join(&name)).unwrap();
            let mut rows: Vec<UnitRow> = text
                .lines()
                .map(|l| serde_json::from_str(l).unwrap())
                .collect();
            drop(text);
            let stats = mark_vendored(&mut rows, &known, &same);
            eprintln!("{slice}: {} rows, marked {stats:?}", rows.len());
            slices.insert(slice.to_string(), stats);
            let mut jsonl = Vec::new();
            for r in &rows {
                serde_json::to_writer(&mut jsonl, r).unwrap();
                jsonl.push(b'\n');
            }
            manifest.tables.insert(name.clone(), hex(&jsonl));
            std::fs::write(out.join(&name), jsonl).unwrap();
            let bin = UnitTable::encode(&rows, &packages).unwrap();
            let bname = format!("units.{slice}.bin");
            manifest.tables.insert(bname.clone(), hex(&bin));
            std::fs::write(out.join(&bname), bin).unwrap();
        } else {
            std::fs::hard_link(src.join(&name), out.join(&name)).unwrap();
        }
    }
    let mut summary = serde_json::to_vec_pretty(&serde_json::json!({
        "about": "Unit rows whose file is a vendored copy of another package (exp115, humanify_core::libindex::vendored): per slice, the rows and files marked.",
        "slices": slices,
    }))
    .unwrap();
    summary.push(b'\n');
    manifest
        .tables
        .insert("vendored.json".into(), hex(&summary));
    std::fs::write(out.join("vendored.json"), summary).unwrap();
    manifest.format = INDEX_FORMAT.to_string();
    let mut m = serde_json::to_vec_pretty(&manifest).unwrap();
    m.push(b'\n');
    std::fs::write(out.join("manifest.json"), &m).unwrap();
    eprintln!("index id {}", hex(&m));
}
