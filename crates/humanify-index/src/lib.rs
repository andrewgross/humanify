//! The offline library index builder (docs/design/library-index.md).
pub mod asof;
pub mod build;
pub mod merge;
pub mod recipe;
pub mod registry;
pub mod shard;
pub mod sourcemap;

#[cfg(test)]
mod asof_test;
#[cfg(test)]
mod build_test;
#[cfg(test)]
mod merge_test;
#[cfg(test)]
mod recipe_test;
#[cfg(test)]
mod shard_test;
#[cfg(test)]
mod sourcemap_test;
