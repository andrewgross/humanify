//! The offline library index builder (docs/design/library-index.md).
pub mod asof;
pub mod build;
pub mod cache;
pub mod merge;
pub mod names;
pub mod recipe;
pub mod registry;
pub mod shard;
pub mod slim;
pub mod sourcemap;

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
mod recipe_test;
#[cfg(test)]
mod shard_test;
#[cfg(test)]
mod slim_test;
#[cfg(test)]
mod sourcemap_test;

/// The recipe the index-crate tests build under (index v2's Bun slice).
#[cfg(test)]
pub(crate) fn bun_node() -> humanify_core::libindex::recipe::Recipe {
    humanify_core::libindex::recipe::Recipe::parse("bun:target=node,format=cjs,minify")
        .expect("a recipe")
}
