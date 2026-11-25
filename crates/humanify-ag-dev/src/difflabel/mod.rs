//! `cargo dev diff label`: every changed line between two output trees
//! gets one label, from what the two inputs and each tree's layout
//! record say about it.

mod label;
mod report;
mod tree;
mod truth;
mod units;

pub use label::{Label, Labeller};
pub use report::{GROUPS, NOT_ADDING_UP, label_trees, read_input, run};
#[cfg(test)]
pub use tree::layout_of_rows;
pub use tree::{Origin, Tree};
pub use truth::{Side, Standing, Truth};
pub use units::InputFile;
