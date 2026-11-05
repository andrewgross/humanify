//! The offline library index builder (docs/design/library-index.md).
pub mod asof;
pub mod build;
pub mod cache;
pub mod cli;
pub mod merge;
pub mod names;
pub mod planner;
pub mod ranges;
pub mod recipe;
pub mod registry;
#[cfg(test)]
mod registry_test;
pub mod shard;
pub mod slim;
pub mod sourcemap;
pub mod spec;
pub mod tools;

#[cfg(test)]
mod asof_test;
#[cfg(test)]
mod build_test;
#[cfg(test)]
mod cache_test;
#[cfg(test)]
mod edges_test;
#[cfg(test)]
mod merge_test;
#[cfg(test)]
mod names_test;
#[cfg(test)]
mod planner_test;
#[cfg(test)]
mod ranges_test;
#[cfg(test)]
mod recipe_test;
#[cfg(test)]
mod shard_test;
#[cfg(test)]
mod slim_test;
#[cfg(test)]
mod sourcemap_test;
#[cfg(test)]
mod spec_test;
#[cfg(test)]
mod tools_test;

/// The recipe the index-crate tests build under (index v2's Bun slice).
#[cfg(test)]
pub(crate) fn bun_node() -> humanify_ag_core::libindex::recipe::Recipe {
    humanify_ag_core::libindex::recipe::Recipe::parse("bun:target=node,format=cjs,minify")
        .expect("a recipe")
}
