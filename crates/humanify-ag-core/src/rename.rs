//! The rename layer: eligibility (WP1.4's early dependency), the naming
//! floor's name shapes and the minifier name profile they are asked
//! under, and validated rename (WP3.1 — the only writer of the name
//! overlay).

pub mod eligibility;
pub mod floor;
pub mod name_profile;
pub mod transfer;
pub mod validated;
pub mod votes;

#[cfg(test)]
mod names_test;
