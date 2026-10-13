//! exp095 "misses", Task 2: claim a statement unit by ONE rare long literal
//! (a string >= 60 chars, a regex >= 40 chars, or a value the literal table
//! keeps as a sha1 because it is > 120 UTF-16 units) that exactly one
//! package FAMILY holds in the index's literal table — the data blobs
//! (wasm, decode tables, emoji regexes) structural units cannot see.
//!
//! Usage: exp095-long-literal <literals.jsonl> <jobs.tsv: query rows, bundle, claims out>
use std::collections::{BTreeSet, HashMap};

use humanify_core::libindex::format::LiteralRow;
use humanify_core::libindex::literals::{LiteralMode, code_literals};
use serde_json::{Value, json};

fn rare(l: &str) -> bool {
    let long = |p: &str, n: usize| l.strip_prefix(p).is_some_and(|v| v.chars().count() >= n);
    l.starts_with("S#")
        || l.starts_with("R#")
        || l.starts_with("P#")
        || long("S:", 60)
        || long("R:", 40)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let families = humanify_core::libindex::families::PackageFamilies::shipped();
    let mut owners: HashMap<String, BTreeSet<String>> = HashMap::new();
    for line in std::fs::read_to_string(&args[1]).expect("literals").lines() {
        let r: LiteralRow = serde_json::from_str(line).expect("row");
        for l in r.lits.iter().filter(|l| rare(l)) {
            owners.entry(l.clone()).or_default().insert(r.owner.clone());
        }
    }
    eprintln!("{} rare literals", owners.len());
    for line in std::fs::read_to_string(&args[2]).expect("jobs").lines() {
        let f: Vec<&str> = line.split('\t').collect();
        let code = std::fs::read_to_string(f[1]).expect("bundle");
        let mut out = String::new();
        for row in std::fs::read_to_string(f[0]).expect("rows").lines() {
            let r: Value = serde_json::from_str(row).unwrap();
            if !matches!(r["k"].as_str(), Some("Statement" | "LazyBody")) {
                continue;
            }
            let (s, e) = (
                r["s"].as_u64().unwrap() as usize,
                r["e"].as_u64().unwrap() as usize,
            );
            let Some(text) = code.get(s..e) else { continue };
            let Some(lits) = code_literals(text, LiteralMode::Bundled) else {
                continue;
            };
            let held: Vec<&BTreeSet<String>> = lits
                .iter()
                .filter(|l| rare(l))
                .filter_map(|l| owners.get(l))
                .collect();
            if held.is_empty() {
                continue;
            }
            let fams: BTreeSet<String> = held
                .iter()
                .flat_map(|o| o.iter().map(|p| families.family(p)))
                .collect();
            if fams.len() != 1 {
                continue;
            }
            let pkg = held[0].iter().next().unwrap();
            out.push_str(&json!({"s": s, "e": e, "package": pkg, "rare": held.len()}).to_string());
            out.push('\n');
        }
        std::fs::write(f[2], out).expect("write");
    }
}
