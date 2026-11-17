//! What reads a scored label: its cards, run records and stats files.

pub mod card;
pub mod difftool;
pub mod llm_diag;
pub mod matching;
pub mod summarize;

#[cfg(test)]
pub(crate) mod test_support;
