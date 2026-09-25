//! The grouping/assignment strategies (WP5.2): fossil-guided
//! ([`fossil`], with its cross-release matcher [`fossil_match`]), the
//! seam-clustered fresh grouping ([`cluster`]), and the LLM namer both use
//! for NEW names ([`namer`]).

pub mod cluster;
pub mod fossil;
pub mod fossil_match;
pub mod namer;
pub mod tokens;
