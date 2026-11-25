//! The split's memory of a tree, read from its layout record.

use std::collections::{BTreeMap, HashMap};

use humanify_ag_model::js::cmp_utf16;

use super::{Layout, Unit};
use crate::place::ledger::StableSplitLedger;

/// Why a layout's split statements give no ledger.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum LedgerViewError {
    #[error(
        "the layout's {statements} split statements are not numbered 0 to {statements} once each"
    )]
    Positions { statements: usize },
    #[error("split statement {n} has no fingerprint")]
    NoFingerprint { n: u32 },
}

/// The statements the split placed, by file in written order.
fn placed(layout: &Layout) -> BTreeMap<&str, Vec<&Unit>> {
    let mut by_file = BTreeMap::new();
    for (path, entry) in &layout.files {
        let units: Vec<&Unit> = entry.units.iter().filter(|u| u.n.is_some()).collect();
        if !units.is_empty() {
            by_file.insert(path.as_str(), units);
        }
    }
    by_file
}

/// The ledger of the tree the layout describes; `None`: the tree was
/// not split.
pub fn ledger_of(layout: &Layout) -> Result<Option<StableSplitLedger>, LedgerViewError> {
    let (Some(hash_version), by_file) = (layout.hash_version, placed(layout)) else {
        return Ok(None);
    };
    let statements: usize = by_file.values().map(Vec::len).sum();
    let mut order = vec![None; statements];
    let mut hashes = vec![String::new(); statements];
    let mut names: Vec<&[String]> = vec![&[]; statements];
    let (mut emit_hashes, mut emit_names) = (hashes.clone(), vec![None; statements]);
    let mut emit_indexes = vec![0u64; statements];
    let bad = LedgerViewError::Positions { statements };
    for (path, units) in &by_file {
        let mut slots: Vec<u32> = units.iter().filter_map(|u| u.n).collect();
        slots.sort_unstable();
        for (unit, slot) in units.iter().zip(slots) {
            let n = unit.n.unwrap_or_default();
            let hash = unit
                .hash
                .clone()
                .ok_or(LedgerViewError::NoFingerprint { n })?;
            let own = order.get_mut(n as usize).filter(|o| o.is_none());
            *own.ok_or_else(|| bad.clone())? = Some((*path).to_string());
            hashes[n as usize].clone_from(&hash);
            names[n as usize] = &unit.names;
            emit_hashes[slot as usize] = hash;
            emit_names[slot as usize] = (!unit.names.is_empty()).then(|| unit.names.join(","));
            emit_indexes[slot as usize] = u64::from(n);
        }
    }
    let order: Vec<String> = order.into_iter().flatten().collect();
    let mut name_to_files: HashMap<String, Vec<String>> = HashMap::new();
    for (file, declared) in order.iter().zip(&names) {
        for name in *declared {
            name_to_files
                .entry(name.clone())
                .or_default()
                .push(file.clone());
        }
    }
    let mut files: Vec<String> = by_file.keys().map(|p| (*p).to_string()).collect();
    files.sort_by(|a, b| cmp_utf16(a, b));
    Ok(Some(StableSplitLedger {
        version: 1,
        files,
        name_to_files,
        order,
        hashes: Some(hashes),
        emit_hashes: Some(emit_hashes),
        emit_names: Some(emit_names),
        emit_indexes: Some(emit_indexes),
        hash_version,
        fossil_modules: layout.modules.clone(),
    }))
}
