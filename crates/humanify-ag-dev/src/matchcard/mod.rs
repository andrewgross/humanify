//! `cargo dev match score`: how many unchanged functions the matcher
//! carried, missed or paired wrongly, before any LLM call.

mod join;
mod report;
mod score;

pub use report::run;
