//! Where each function of a named text sits in its version's input.

use std::collections::BTreeMap;

use super::score::Located;
use crate::difflabel::{Origin, Side, Standing, Tree};

pub const NAMED_TEXT: &str = ".humanify/humanified.js";

/// What the input comparison says about a side's functions, by count.
pub type Standings = BTreeMap<&'static str, u64>;

/// Per input file, every byte a row of the named text starts at, sorted.
fn row_bytes(tree: &Tree) -> BTreeMap<u32, Vec<u32>> {
    let mut out: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for (_, origin) in tree.rows(NAMED_TEXT) {
        if let Origin::At(file, offset) = origin {
            out.entry(*file).or_default().push(*offset);
        }
    }
    for bytes in out.values_mut() {
        bytes.sort_unstable();
        bytes.dedup();
    }
    out
}

/// The input byte of each function of one row, in text order: each
/// function's own start when the input holds as many functions between
/// this row's byte and the next row's as the row does, else the row's
/// byte for all of them.
fn bytes_in_row(
    side: &Side,
    file: &str,
    (from, to): (u32, u32),
    functions: usize,
) -> (Vec<u32>, bool) {
    let own = side.function_starts(file, from, to);
    if own.len() == functions {
        (own, true)
    } else {
        (vec![from; functions], false)
    }
}

pub struct LocatedSide {
    pub functions: Vec<Located>,
    pub standings: Standings,
    /// Functions located by their own first byte.
    pub by_own_byte: u64,
}

/// One `Located` per function, given each function's line in the named
/// text, in text order.
pub fn locate(tree: &Tree, side: &Side, lines: &[u32]) -> LocatedSide {
    let rows = tree.rows(NAMED_TEXT);
    let row_bytes = row_bytes(tree);
    let row_of = |line: u32| rows.partition_point(|r| r.0 <= line);
    let mut out = LocatedSide {
        functions: Vec::with_capacity(lines.len()),
        standings: Standings::new(),
        by_own_byte: 0,
    };
    for group in lines.chunk_by(|a, b| row_of(*a) == row_of(*b)) {
        let origin = row_of(group[0]).checked_sub(1).map(|r| rows[r].1);
        let at = match origin {
            Some(Origin::At(file, offset)) => {
                let bytes = &row_bytes[&file];
                let next = bytes.partition_point(|b| *b <= offset);
                let to = bytes.get(next).copied().unwrap_or(u32::MAX);
                let name = tree.input_name(file);
                let (bytes, own) = bytes_in_row(side, name, (offset, to), group.len());
                out.by_own_byte += if own { group.len() as u64 } else { 0 };
                Some((name, bytes))
            }
            _ => None,
        };
        for k in 0..group.len() {
            let i = out.functions.len() as u64;
            let byte = at.as_ref().map(|(_, bytes)| bytes[k]);
            let standing = at.as_ref().map(|(name, bytes)| side.at(name, bytes[k]));
            *out.standings.entry(standing_name(standing)).or_default() += 1;
            out.functions.push(Located {
                pair: match standing {
                    Some(Standing::Unchanged(pair)) => Some(pair),
                    _ => None,
                },
                order: byte.map_or(0, |b| u64::from(b) << 32) | i,
            });
        }
    }
    out
}

/// One `Located` per function: an init wrapper the linker wrote sits in
/// the pair `pair_of` gives its input file; every other function in none.
pub fn locate_init_wrappers(
    tree: &Tree,
    lines: &[u32],
    pair_of: &dyn Fn(&str) -> Option<u32>,
) -> Vec<Located> {
    lines
        .iter()
        .enumerate()
        .map(|(i, line)| {
            let file = match tree.origin(NAMED_TEXT, *line) {
                Some(Origin::Linker(file)) if is_init_wrapper(tree.line(NAMED_TEXT, *line)) => {
                    Some(file)
                }
                _ => None,
            };
            Located {
                pair: file.and_then(|f| pair_of(tree.input_name(f))),
                order: i as u64,
            }
        })
        .collect()
}

/// `var <name> = <helper>(() => {` or its async form.
fn is_init_wrapper(line: Option<&str>) -> bool {
    let Some(line) = line.map(str::trim) else {
        return false;
    };
    let opens = ["(() => {", "(async () => {"];
    line.starts_with("var ")
        && opens
            .iter()
            .any(|open| line.ends_with(open) || line.ends_with(&format!("{open}}});")))
}

fn standing_name(standing: Option<Standing>) -> &'static str {
    match standing {
        None => "no input byte",
        Some(Standing::Unchanged(_)) => "unchanged",
        Some(Standing::Duplicate) => "duplicate",
        Some(Standing::Changed) => "changed",
        Some(Standing::OutsideUnits) => "outside units",
        Some(Standing::UnknownFile) => "unknown input file",
    }
}

#[cfg(test)]
#[path = "join_test.rs"]
mod join_test;
