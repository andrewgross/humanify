//! `cargo dev diff label`: every changed line between two output trees
//! gets one label, from what the two inputs and each tree's provenance
//! file say about it.

mod label;
mod report;
mod tree;
mod truth;
mod units;

pub use label::{Label, Labeller};
pub use report::run;
pub use tree::{Origin, Tree};
pub use truth::{Standing, Truth};
pub use units::InputFile;
