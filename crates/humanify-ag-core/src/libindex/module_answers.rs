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
//! dropped): the reader answers it with both owners, each with its own
//! files, and the planner decides whether the claim is one of them
//! (exp211; before, both owners and no file, always a disagreement).
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

/// One key's rows into one answer: every holding package with ITS files
/// (sorted and de-duplicated — the builder sorts, this re-sorts so the
/// planner's contract holds whatever the file says). Two or more holders
/// keep every owner's files (exp211): whether the claim is one of them is
/// the planner's question, not the reader's.
fn answer_of(rows: Vec<ModuleRow>) -> ModuleAnswer {
    let mut owners: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for row in rows {
        owners.entry(row.owner).or_default().extend(row.files);
    }
    for files in owners.values_mut() {
        files.sort();
        files.dedup();
    }
    ModuleAnswer { owners }
}
