//! The names-mode module-answer reader (library-names plan step 4,
//! exp156): the `modules.<slice>.jsonl` rows the index builder writes and
//! nothing read before now, streamed once into the planner's per-module
//! answers.
//!
//! The rows are keyed by the whole-module VENDOR CONTENT key
//! (`modules::vendor_content`), which is the same form at both ends
//! because the builder fingerprints with this crate's own code — and the
//! manifest was already stamp-checked when [`super::lookup::LibraryIndex`]
//! was loaded, so the rows here are this fingerprint era's. A key held by
//! two packages is TWO rows (build-time collisions are evidence, never
//! dropped): the reader answers it with both owners and no files, which
//! the planner reads as a disagreement and leaves in place.
//!
//! A missing `modules.<slice>.jsonl` when a key was asked for is a
//! REFUSAL, never an empty answer — a broken or half-written index must
//! not masquerade as recorded silence (the exp155 callee-prints rule).

use std::collections::{BTreeMap, BTreeSet};
use std::io::BufRead;
use std::path::Path;

use super::format::ModuleRow;
use crate::place::library_layout::ModuleAnswer;

/// `modules.<slice>.jsonl` — the builder's name for a slice's module rows.
pub fn modules_file(slice: &str) -> String {
    format!("modules.{slice}.jsonl")
}

/// The planner's answers for `keys`: one entry per key the index holds.
/// Keys the index lacks are simply absent (a miss is a synthetic name,
/// not an error). An empty `keys` reads nothing.
pub fn read_module_answers(
    dir: &Path,
    slice: &str,
    keys: &BTreeSet<String>,
) -> Result<BTreeMap<String, ModuleAnswer>, String> {
    if keys.is_empty() {
        return Ok(BTreeMap::new());
    }
    let file = modules_file(slice);
    let path = dir.join(&file);
    let handle = std::fs::File::open(&path).map_err(|e| format!("{file}: {e}"))?;
    let mut rows: BTreeMap<String, Vec<ModuleRow>> = BTreeMap::new();
    for (n, line) in std::io::BufReader::new(handle).lines().enumerate() {
        let line = line.map_err(|e| format!("{file}: {e}"))?;
        if line.is_empty() {
            continue;
        }
        let row: ModuleRow =
            serde_json::from_str(&line).map_err(|e| format!("{file} line {}: {e}", n + 1))?;
        if keys.contains(&row.key) {
            let key = row.key.clone();
            rows.entry(key).or_default().push(row);
        }
    }
    Ok(rows
        .into_iter()
        .map(|(key, rows)| (key, answer_of(rows)))
        .collect())
}

/// One key's rows into one answer: a single holding package answers with
/// its files (sorted — the builder sorts, this re-sorts so the planner's
/// contract holds whatever the file says); two or more holders answer
/// with every owner and NO file, the planner's foreign case.
fn answer_of(mut rows: Vec<ModuleRow>) -> ModuleAnswer {
    if rows.len() == 1 {
        let row = rows.pop().expect("answer_of called with one row");
        let mut files = row.files;
        files.sort();
        return ModuleAnswer {
            owners: vec![row.owner],
            files,
        };
    }
    let mut owners: Vec<String> = rows.into_iter().map(|r| r.owner).collect();
    owners.sort();
    owners.dedup();
    ModuleAnswer {
        owners,
        files: Vec::new(),
    }
}
