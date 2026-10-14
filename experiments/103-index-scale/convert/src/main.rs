//! exp103 converter: `exp103-convert <index /2 dir> <out dir>`.
//!
//! Writes the /3 index the MERGE would have written from the same rows:
//! the row tables copied byte for byte (units.<slice>.jsonl,
//! modules.<slice>.jsonl, versions.json, the data files), the literal rows
//! split into the lookup table + `literals.files.jsonl`, the lookup tables
//! (`humanify_core::libindex::table::lookup_files`, the merge's own call),
//! and the manifest with the /3 format and every table's sha256 (the
//! merge's rule: every file but the data files and the manifest).

use std::collections::BTreeMap;
use std::path::Path;

use humanify_core::libindex::format::{
    INDEX_FORMAT, LiteralFileRow, LiteralRow, Manifest, UnitRow,
};
use humanify_core::libindex::table::lookup_files;
use sha2::{Digest, Sha256};

fn hex_sha256(b: &[u8]) -> String {
    Sha256::digest(b)
        .iter()
        .map(|x| format!("{x:02x}"))
        .collect()
}

fn rows<T: serde::de::DeserializeOwned>(text: &str) -> Vec<T> {
    text.lines()
        .map(|l| serde_json::from_str(l).expect("a row"))
        .collect()
}

fn read(dir: &Path, name: &str) -> Vec<u8> {
    std::fs::read(dir.join(name)).unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (src, out) = (Path::new(&args[1]), Path::new(&args[2]));
    assert!(
        !out.exists(),
        "{} exists: the converter writes a NEW directory",
        out.display()
    );
    let mut manifest: Manifest =
        serde_json::from_slice(&read(src, "manifest.json")).expect("manifest");
    assert_eq!(
        manifest.format, "humanify-library-index/2",
        "converts /2 only"
    );
    let mut files: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let mut units: BTreeMap<String, Vec<UnitRow>> = BTreeMap::new();
    for entry in std::fs::read_dir(src).expect("dir") {
        let name = entry
            .expect("entry")
            .file_name()
            .into_string()
            .expect("utf8");
        if name.starts_with("modules.") || name == "versions.json" {
            files.insert(name.clone(), read(src, &name));
        } else if let Some(slice) = name
            .strip_prefix("units.")
            .and_then(|n| n.strip_suffix(".jsonl"))
        {
            let bytes = read(src, &name);
            units.insert(
                slice.to_string(),
                rows(std::str::from_utf8(&bytes).expect("utf8")),
            );
            files.insert(name.clone(), bytes);
        }
    }
    let literal_bytes = read(src, "literals.jsonl");
    let literals: Vec<LiteralRow> = rows(std::str::from_utf8(&literal_bytes).expect("utf8"));
    drop(literal_bytes);
    files.extend(lookup_files(&units, &literals).expect("encodes"));
    let mut provenance = Vec::new();
    for r in literals {
        let row = LiteralFileRow {
            owner: r.owner,
            file: r.file,
            versions: r.versions,
        };
        provenance.extend(serde_json::to_vec(&row).expect("json"));
        provenance.push(b'\n');
    }
    files.insert("literals.files.jsonl".into(), provenance);
    manifest.format = INDEX_FORMAT.to_string();
    manifest.tables = files
        .iter()
        .map(|(n, b)| (n.clone(), hex_sha256(b)))
        .collect();
    for name in manifest.data.keys() {
        files.insert(name.clone(), read(src, name));
    }
    let mut m = serde_json::to_vec_pretty(&manifest).expect("json");
    m.push(b'\n');
    let id = hex_sha256(&m);
    files.insert("manifest.json".into(), m);
    std::fs::create_dir_all(out).expect("mkdir");
    for (name, bytes) in &files {
        std::fs::write(out.join(name), bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
    }
    println!("index id {id}");
}
