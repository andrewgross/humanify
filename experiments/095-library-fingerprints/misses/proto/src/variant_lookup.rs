//! exp095 "misses", Task 2: the pipeline's OWN claim rule
//! (`libindex::lookup::claim_units`: structural + specificity + support,
//! optionally literal agreement) over an index built from
//! `exp095-variant-rows` output, with the prints of one variant.
//!
//! Usage: exp095-variant-lookup <variant> <index rows dir> <literals.jsonl|-> <jobs.tsv>
//!   variant: base | norm | normbind | bind (= base whole+functions + normalised bindings)
//!   jobs.tsv: query rows (exp095-variant-rows, query side)<TAB>bundle.js<TAB>claims out
//!   env: AGREEMENT=1, MIN_NODES, SUPPORT, SPEC=0, MAX_PACKAGES
//! Claims out: one JSON row per claim {s, e, package, structural, literal}.
use std::collections::HashSet;

use humanify_core::libindex::format::{LiteralRow, UnitRow};
use humanify_core::libindex::lookup::{LibraryIndex, LookupRules, UnitQuery, claim_units};
use humanify_core::libindex::units::{StatementPrints, UnitPrint};
use oxc_span::Span;
use serde_json::{Value, json};

fn print_of(v: &Value) -> UnitPrint {
    UnitPrint {
        span: Span::new(v[0].as_u64().unwrap() as u32, v[1].as_u64().unwrap() as u32),
        hash: v[2].as_str().unwrap().to_string(),
        nodes: v[3].as_u64().unwrap() as u32,
        content: v[4].as_u64().unwrap() as u32,
    }
}

/// The query prints of one unit under `variant`: (whole, extra prints).
fn query_prints(row: &Value, variant: &str) -> StatementPrints {
    let base: Vec<UnitPrint> = row["base"]
        .as_array()
        .unwrap()
        .iter()
        .map(print_of)
        .collect();
    let norm: Vec<UnitPrint> = row["norm"]
        .as_array()
        .unwrap()
        .iter()
        .map(print_of)
        .collect();
    let bind: Vec<UnitPrint> = row["bind"]
        .as_array()
        .unwrap()
        .iter()
        .map(print_of)
        .collect();
    let (whole, mut functions) = match variant {
        "norm" | "normbind" => (norm[0].clone(), norm[1..].to_vec()),
        _ => (base[0].clone(), base[1..].to_vec()),
    };
    if variant == "normbind" || variant == "bind" {
        let have: HashSet<(u32, u32)> = functions
            .iter()
            .map(|f| (f.span.start, f.span.end))
            .collect();
        functions.extend(
            bind.into_iter()
                .filter(|b| !have.contains(&(b.span.start, b.span.end)))
                .filter(|b| (b.span.start, b.span.end) != (whole.span.start, whole.span.end)),
        );
    }
    StatementPrints { whole, functions }
}

fn index_rows(dir: &str, variant: &str) -> Vec<UnitRow> {
    let mut seen: HashSet<(String, String, u64)> = HashSet::new();
    let mut out = Vec::new();
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .expect("rows dir")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    files.sort();
    for f in files {
        let Ok(text) = std::fs::read_to_string(&f) else {
            continue;
        };
        for line in text.lines() {
            let r: Value = serde_json::from_str(line).expect("row");
            let owner = r["owner"].as_str().unwrap();
            let mb = r["mb"].as_u64().unwrap_or(0);
            let mut push = |p: &Value| {
                let h = p[0].as_str().unwrap().to_string();
                if seen.insert((h.clone(), owner.to_string(), mb)) {
                    out.push(UnitRow {
                        hash: h,
                        owner: owner.to_string(),
                        file: r["file"].as_str().unwrap_or("").to_string(),
                        nodes: p[1].as_u64().unwrap() as u32,
                        content: p[2].as_u64().unwrap() as u32,
                        module_bytes: mb,
                        versions: vec![],
                    });
                }
            };
            let key = match variant {
                "norm" | "normbind" => "norm",
                _ => "base",
            };
            for p in r[key].as_array().into_iter().flatten() {
                push(p);
            }
            if variant == "normbind" || variant == "bind" {
                for p in r["bind"].as_array().into_iter().flatten() {
                    push(p);
                }
            }
        }
    }
    out
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (variant, dir, lits, jobs) = (&args[1], &args[2], &args[3], &args[4]);
    let flag = |k: &str| std::env::var(k).ok();
    let t = std::time::Instant::now();
    let units = index_rows(dir, variant);
    let literals: Vec<LiteralRow> = if lits == "-" {
        Vec::new()
    } else {
        std::fs::read_to_string(lits)
            .expect("literals")
            .lines()
            .map(|l| serde_json::from_str(l).expect("literal row"))
            .collect()
    };
    eprintln!(
        "{} unit rows, {} literal rows, read in {:.0}s",
        units.len(),
        literals.len(),
        t.elapsed().as_secs_f64()
    );
    let index = LibraryIndex::from_rows(units, literals);
    let mut rules = LookupRules {
        agreement: flag("AGREEMENT").as_deref() == Some("1"),
        ..LookupRules::default()
    };
    if let Some(n) = flag("MIN_NODES") {
        rules.min_nodes = n.parse().unwrap();
    }
    if let Some(n) = flag("SUPPORT") {
        rules.support = n.parse().unwrap();
    }
    if let Some(n) = flag("MAX_PACKAGES") {
        rules.max_packages = n.parse().unwrap();
    }
    if flag("SPEC").as_deref() == Some("0") {
        rules.specificity = None;
    }
    let families = humanify_core::libindex::families::PackageFamilies::shipped();
    let family = |p: &str| families.family(p);
    for line in std::fs::read_to_string(jobs).expect("jobs").lines() {
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() < 3 {
            continue;
        }
        let code = std::fs::read_to_string(f[1]).expect("bundle");
        let rows: Vec<Value> = std::fs::read_to_string(f[0])
            .expect("query rows")
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .filter(|r: &Value| matches!(r["k"].as_str(), Some("Statement" | "LazyBody")))
            .collect();
        let prints: Vec<StatementPrints> = rows.iter().map(|r| query_prints(r, variant)).collect();
        let queries: Vec<UnitQuery<'_>> = rows
            .iter()
            .zip(&prints)
            .enumerate()
            .map(|(i, (r, p))| UnitQuery {
                unit: i as u32,
                prints: p,
                module_bytes: r["mb"].as_u64().unwrap(),
                text: code
                    .get(r["s"].as_u64().unwrap() as usize..r["e"].as_u64().unwrap() as usize)
                    .unwrap_or(""),
            })
            .collect();
        let claims = claim_units(&index, &queries, rules, &family);
        let out: String = claims
            .iter()
            .map(|c| {
                let r = &rows[c.unit as usize];
                json!({"s": r["s"], "e": r["e"], "package": c.package, "structural": c.structural,
                       "literal": c.literal.as_ref().map(|l| l.1)})
                .to_string()
                    + "\n"
            })
            .collect();
        std::fs::write(f[2], out).expect("claims");
        eprintln!("{}: {} claims", f[1], claims.len());
    }
}
