//! The parity differ's CLI (docs/rust-port/07-differential-validation.md §4):
//!
//!   humanify-parity compare <ts-dump> <rust-dump> [--sections a,b] [--max-divergences N]
//!   humanify-parity compare-ledger <a>/split-ledger.json <b>/split-ledger.json
//!   humanify-parity selftest
//!
//! Exit codes: 0 identical, 1 divergences, 2 not comparable. Every gate is
//! exact set or byte equality — no tolerance parameter exists (07 §9).

use clap::{Parser, Subcommand};
use humanify_parity::{engine, selftest};

#[derive(Parser)]
#[command(name = "humanify-parity", version, about = "The parity differ (07 §4)")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Compare two artifact dumps section by section.
    Compare {
        /// Left dump directory (the oracle side).
        left: String,
        /// Right dump directory (the candidate side).
        right: String,
        /// Comma-separated section list (default: all).
        #[arg(long)]
        sections: Option<String>,
        /// Cap on reported divergences.
        #[arg(long, default_value_t = 20)]
        max_divergences: usize,
    },
    /// Compare two split ledgers: hash-bearing fields as partitions,
    /// everything else byte-exact (07 §6).
    CompareLedger {
        left: String,
        right: String,
        #[arg(long, default_value_t = 20)]
        max_divergences: usize,
    },
    /// Prove the instrument can fail: planted divergences must be detected.
    Selftest,
}

fn main() {
    let cli = Cli::parse();
    let code = match cli.command {
        Command::Selftest => match selftest::run_selftest() {
            Ok(()) => {
                println!(
                    "selftest: every planted divergence detected (control identical, {} planted cases + control)",
                    selftest::planted_case_count()
                );
                0
            }
            Err(msg) => {
                eprintln!("SELFTEST FAILED: {msg}");
                1
            }
        },
        Command::Compare {
            left,
            right,
            sections,
            max_divergences,
        } => run_compare(
            &left,
            &right,
            sections.as_deref().unwrap_or(""),
            max_divergences,
        ),
        Command::CompareLedger {
            left,
            right,
            max_divergences,
        } => run_compare_ledger(&left, &right, max_divergences),
    };
    std::process::exit(code);
}

fn run_compare(left: &str, right: &str, sections_spec: &str, max_divergences: usize) -> i32 {
    let sections = engine::parse_sections(sections_spec);
    let outcome = match engine::compare_dumps(
        std::path::Path::new(left),
        std::path::Path::new(right),
        &sections,
        max_divergences,
    ) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("NOT COMPARABLE: {e}");
            return 2;
        }
    };
    if let Some(reason) = &outcome.not_comparable_reason {
        eprintln!("NOT COMPARABLE: {reason}");
        return 2;
    }
    if outcome.divergences.is_empty() {
        println!(
            "IDENTICAL: {} section(s) compared clean across {left} vs {right}",
            sections.len()
        );
        return 0;
    }
    println!(
        "DIVERGED: {} divergence(s) across {left} vs {right}:",
        outcome.divergences.len()
    );
    for d in &outcome.divergences {
        println!("  [{}:{}] {}", d.section, d.kind, d.key);
        if let Some(l) = &d.left {
            println!("    left:  {l}");
        }
        if let Some(r) = &d.right {
            println!("    right: {r}");
        }
    }
    1
}

/// compare-ledger: hash-bearing fields as partitions, everything else
/// byte-exact. Hash fields: `hashes`, `emitHashes`, `fossilModules[].hashes`,
/// `fossilModules[].tokens` (stable-split.ts:207-312).
fn run_compare_ledger(left: &str, right: &str, max_divergences: usize) -> i32 {
    let load = |p: &str| -> Result<serde_json::Value, String> {
        let text = std::fs::read_to_string(p).map_err(|e| format!("{p}: {e}"))?;
        serde_json::from_str(&text).map_err(|e| format!("{p}: {e}"))
    };
    let (lv, rv) = match (load(left), load(right)) {
        (Ok(l), Ok(r)) => (l, r),
        _ => {
            eprintln!("NOT COMPARABLE: a ledger file is missing or unparseable");
            return 2;
        }
    };
    let mut divergences: Vec<String> = Vec::new();

    // Hash-bearing fields as partitions (slot -> representative).
    for field in ["hashes", "emitHashes"] {
        let l = lv.get(field).and_then(|v| v.as_array());
        let r = rv.get(field).and_then(|v| v.as_array());
        match (l, r) {
            (Some(l), Some(r)) => {
                if partition_of_slots(l) != partition_of_slots(r) {
                    divergences.push(format!("{field}: partition differs"));
                }
            }
            _ => divergences.push(format!("{field}: missing on one side")),
        }
    }
    // fossilModules[].hashes/tokens as partitions per module; the rest exact.
    let fossil_partition = |v: &serde_json::Value| -> Option<Vec<Vec<String>>> {
        v.get("fossilModules")
            .and_then(|f| f.as_array())
            .map(|mods| {
                mods.iter()
                    .map(|m| {
                        let mut parts: Vec<String> = Vec::new();
                        for field in ["hashes", "tokens"] {
                            if let Some(arr) = m.get(field).and_then(|x| x.as_array()) {
                                parts.extend(
                                    arr.iter().filter_map(|h| h.as_str()).map(String::from),
                                );
                            }
                        }
                        parts.sort();
                        parts
                    })
                    .collect::<Vec<_>>()
            })
    };
    match (fossil_partition(&lv), fossil_partition(&rv)) {
        (Some(l), Some(r)) if l != r => {
            divergences.push("fossilModules: hash/token partitions differ".to_string())
        }
        (Some(_), Some(_)) => {}
        _ => divergences.push("fossilModules: missing on one side".to_string()),
    }

    // Everything else byte-exact: clone both, strip the hash-bearing fields.
    let strip = |v: &serde_json::Value| -> serde_json::Value {
        let mut v = v.clone();
        if let Some(obj) = v.as_object_mut() {
            obj.remove("hashes");
            obj.remove("emitHashes");
            obj.remove("hashVersion");
            if let Some(mods) = obj.get_mut("fossilModules").and_then(|f| f.as_array_mut()) {
                for m in mods {
                    if let Some(m) = m.as_object_mut() {
                        m.remove("hashes");
                        m.remove("tokens");
                    }
                }
            }
        }
        v
    };
    if strip(&lv) != strip(&rv) {
        divergences.push("non-hash fields differ (byte-exact comparison)".to_string());
    }

    if divergences.is_empty() {
        println!("LEDGER IDENTICAL: {left} vs {right}");
        0
    } else {
        println!(
            "LEDGER DIVERGED: {} divergence(s) across {left} vs {right}:",
            divergences.len()
        );
        for d in divergences.iter().take(max_divergences) {
            println!("  {d}");
        }
        1
    }
}

/// Slot -> representative (the smallest slot index sharing that hash).
fn partition_of_slots(hashes: &[serde_json::Value]) -> Vec<(usize, usize)> {
    let mut reps: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    let mut out = Vec::new();
    for (slot, h) in hashes.iter().enumerate() {
        let key = h.as_str().unwrap_or_default().to_string();
        let next = reps.entry(key).or_insert(slot);
        out.push((slot, *next));
    }
    out
}
