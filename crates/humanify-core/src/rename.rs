//! The rename layer: eligibility (WP1.4's early dependency), the naming
//! floor's name shapes, and validated rename (WP3.1 — the only writer of
//! the name overlay).

pub mod eligibility;
pub mod floor;
pub mod validated;

#[cfg(test)]
mod names_test;
