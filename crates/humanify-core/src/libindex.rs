//! The library fingerprint index: what the pipeline and the offline index
//! builder (`crates/humanify-index`) share (docs/design/library-index.md,
//! exp095, finding #98).
//!
//! The builder runs outside a humanify run — it installs and bundles npm
//! packages — but every fingerprint it writes is computed HERE, by the
//! code the pipeline will call to look a module up, so the two can never
//! disagree about what a fingerprint is:
//!
//! - the whole-module key of a bundled CommonJS module is the vendor
//!   content key (`modules::vendor_content::bundle_content_keys`);
//! - the literal set is [`literals`] (`Source` mode for npm files,
//!   `Bundled` mode for a module of a bundle);
//! - the on-disk shapes, the version order and the version ranges are
//!   [`format`]; the tables a run looks up in (read on demand, never
//!   parsed on load) are [`table`].
//!
//! Which part of the index a bundle is looked up in is its build
//! [`recipe::Recipe`] — an index is a set of recipe slices, and a run
//! picks one from what detection read off the bundle
//! (docs/library-matching-config.md §3).

pub mod callctx;
pub mod families;
pub mod format;
pub mod literals;
pub mod lookup;
pub mod recipe;
pub mod table;
pub mod units;

#[cfg(test)]
#[path = "libindex/callctx_test.rs"]
mod callctx_test;
#[cfg(test)]
#[path = "libindex/families_test.rs"]
mod families_test;
#[cfg(test)]
#[path = "libindex/format_test.rs"]
mod format_test;
#[cfg(test)]
#[path = "libindex/literals_test.rs"]
mod literals_test;
#[cfg(test)]
#[path = "libindex/lookup_test.rs"]
mod lookup_test;
#[cfg(test)]
#[path = "libindex/recipe_test.rs"]
mod recipe_test;
#[cfg(test)]
#[path = "libindex/table_test.rs"]
mod table_test;
#[cfg(test)]
#[path = "libindex/units_test.rs"]
mod units_test;
