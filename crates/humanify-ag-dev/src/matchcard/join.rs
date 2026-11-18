//! Where each function of a named text sits in its version's input.

use std::collections::BTreeMap;

use super::score::Located;
use crate::difflabel::{Origin, Side, Standing, Tree};

pub const NAMED_TEXT: &str = ".humanify/humanified.js";

/// What the input comparison says about a side's functions, by count.
pub type Standings = BTreeMap<&'static str, u64>;

/// One `Located` per function, given each function's line in the named
/// text.
pub fn locate(tree: &Tree, side: &Side, lines: &[u32]) -> (Vec<Located>, Standings) {
    let mut standings = Standings::new();
    let located = lines
        .iter()
        .enumerate()
        .map(|(i, line)| {
            let at = match tree.origin(NAMED_TEXT, *line) {
                Some(Origin::At(file, offset)) => Some((file, offset)),
                _ => None,
            };
            let standing = at.map(|(file, offset)| side.at(tree.input_name(file), offset));
            *standings.entry(standing_name(standing)).or_default() += 1;
            Located {
                pair: match standing {
                    Some(Standing::Unchanged(pair)) => Some(pair),
                    _ => None,
                },
                order: at.map_or(0, |(_, offset)| u64::from(offset) << 32) | i as u64,
            }
        })
        .collect();
    (located, standings)
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
