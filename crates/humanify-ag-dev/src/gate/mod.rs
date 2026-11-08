//! The gate (`cargo dev check`) and the small tools it and the commit hook
//! run (lane L4, docs/dev-harness.md §4 d).

pub mod check;
pub mod env_check;
pub mod format_golden;
pub mod precommit;
pub mod private_addresses;
pub mod shims;
