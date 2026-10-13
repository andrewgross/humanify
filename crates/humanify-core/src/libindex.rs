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
//!   [`format`].
//!
//! Which part of the index a bundle is looked up in is the bundler
//! [`IndexSlice`] — a plugin piece: the one place a slice maps to a
//! bundler (docs/plugin-spec.md).

use humanify_model::detection::BundlerType;

pub mod families;
pub mod format;
pub mod literals;
pub mod lookup;
pub mod units;

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
#[path = "libindex/units_test.rs"]
mod units_test;

/// The bundler family a structural fingerprint was built with. Literals are
/// bundler-independent and shared by every slice.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum IndexSlice {
    /// Bun builds (one slice for the Bun family; the exp095 normaliser is
    /// what makes Bun versions agree).
    Bun,
    /// esbuild builds.
    Esbuild,
}

impl IndexSlice {
    /// Every slice, in table order.
    pub const ALL: [IndexSlice; 2] = [IndexSlice::Bun, IndexSlice::Esbuild];

    /// The recorded spelling (table and shard directory names).
    pub fn name(self) -> &'static str {
        match self {
            IndexSlice::Bun => "bun",
            IndexSlice::Esbuild => "esbuild",
        }
    }

    /// The slice a recorded spelling names.
    pub fn from_name(name: &str) -> Option<IndexSlice> {
        IndexSlice::ALL.into_iter().find(|s| s.name() == name)
    }

    /// The bundler a slice is built with (the builder hands it to
    /// `toolchain::resolve_toolchain` as the bundler flag, so the index's
    /// module walk uses the same pieces a run on that bundler would).
    pub fn bundler(self) -> BundlerType {
        match self {
            IndexSlice::Bun => BundlerType::Bun,
            IndexSlice::Esbuild => BundlerType::Esbuild,
        }
    }

    /// The slice a bundle from `bundler` is looked up in; None: no
    /// structural slice exists for it (literal lookups only).
    pub fn for_bundler(bundler: BundlerType) -> Option<IndexSlice> {
        match bundler {
            BundlerType::Bun => Some(IndexSlice::Bun),
            BundlerType::Esbuild => Some(IndexSlice::Esbuild),
            _ => None,
        }
    }
}
