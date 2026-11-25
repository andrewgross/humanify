//! Where each function of a tree's named text sits in its version's input.

use std::collections::BTreeMap;

use humanify_ag_core::layout_record::{Function, FunctionId, Layout};

use super::score::Located;
use crate::difflabel::{Side, Standing};

/// What the input comparison says about a side's functions, by count.
pub type Standings = BTreeMap<&'static str, u64>;

pub struct LocatedSide {
    pub functions: Vec<Located>,
    pub standings: Standings,
    /// Functions the layout names by an input byte.
    pub by_own_byte: u64,
}

/// The functions of the layout's named text, in written order.
pub fn functions(layout: &Layout) -> Vec<Function<'_>> {
    let named = layout.files.get(&layout.named_text);
    named.into_iter().flat_map(|e| e.functions()).collect()
}

fn input_name(layout: &Layout, input: u32) -> &str {
    layout.inputs.get(input as usize).map_or("", String::as_str)
}

/// One `Located` per function of the named text.
pub fn locate(layout: &Layout, side: &Side) -> LocatedSide {
    let mut out = LocatedSide {
        functions: Vec::new(),
        standings: Standings::new(),
        by_own_byte: 0,
    };
    for (i, function) in functions(layout).iter().enumerate() {
        let name = input_name(layout, function.input);
        let byte = match function.id {
            FunctionId::Byte(byte) => Some(side.function_from(name, *byte).unwrap_or(*byte)),
            FunctionId::Linker(_) | FunctionId::Unknown => None,
        };
        out.by_own_byte += u64::from(byte.is_some());
        let standing = byte.map(|b| side.at(name, b));
        *out.standings.entry(standing_name(standing)).or_default() += 1;
        out.functions.push(Located {
            pair: match standing {
                Some(Standing::Unchanged(pair)) => Some(pair),
                _ => None,
            },
            order: byte.map_or(0, |b| u64::from(b) << 32) | i as u64,
        });
    }
    out
}

/// The function a startup function the linker wrote is.
pub fn is_init_wrapper(function: &Function<'_>) -> bool {
    matches!(function.id, FunctionId::Linker(id) if id.starts_with("init+"))
}

/// One `Located` per function: an init wrapper the linker wrote sits in
/// the pair `pair_of` gives its input file; every other function in none.
pub fn locate_init_wrappers(
    layout: &Layout,
    pair_of: &dyn Fn(&str) -> Option<u32>,
) -> Vec<Located> {
    functions(layout)
        .iter()
        .enumerate()
        .map(|(i, function)| Located {
            pair: is_init_wrapper(function)
                .then(|| pair_of(input_name(layout, function.input)))
                .flatten(),
            order: i as u64,
        })
        .collect()
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
pub mod join_test;
