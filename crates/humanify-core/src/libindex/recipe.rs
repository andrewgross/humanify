//! Index slices are BUILD RECIPES (docs/library-matching-config.md §3,
//! Andrew 2026-10-13): the packing tool decides what gets bundled — Bun
//! `--target bun` resolves a package's `module` build (`dist-es`) where
//! `--target node` resolves `main` (`dist-cjs`), and escapes non-ASCII
//! strings differently — so an index is a set of recipe slices, building
//! one names its recipes, and a run picks the slice from what detection
//! read off the bundle, ONCE ([`detect_recipe`] + [`choose_slice`]).
//!
//! A recipe's spelling (`bun:target=bun,format=cjs,minify`) is what the
//! builder's `--recipe` takes and the manifest records; its slice name
//! (`bun-bun-cjs-min`) names the tables (`units.<slice>.jsonl`), the shard
//! directories and the build directories.

use humanify_model::detection::{BundlerType, MinifierType};

/// The bundler a recipe runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RecipeBundler {
    Bun,
    Esbuild,
    /// webpack 5 + its production minifier (terser) — exp105.
    Webpack,
    /// rollup 4 + node-resolve + commonjs + json + terser — exp105.
    Rollup,
}

/// Every recipe bundler.
const RECIPE_BUNDLERS: [RecipeBundler; 4] = [
    RecipeBundler::Bun,
    RecipeBundler::Esbuild,
    RecipeBundler::Webpack,
    RecipeBundler::Rollup,
];

impl RecipeBundler {
    pub fn name(self) -> &'static str {
        match self {
            RecipeBundler::Bun => "bun",
            RecipeBundler::Esbuild => "esbuild",
            RecipeBundler::Webpack => "webpack",
            RecipeBundler::Rollup => "rollup",
        }
    }

    fn from_name(name: &str) -> Option<RecipeBundler> {
        RECIPE_BUNDLERS.into_iter().find(|b| b.name() == name)
    }

    /// The pipeline's bundler type (the builder hands it to
    /// `toolchain::resolve_toolchain`, so the index's module walk uses the
    /// pieces a run on that bundler would).
    pub fn bundler_type(self) -> BundlerType {
        match self {
            RecipeBundler::Bun => BundlerType::Bun,
            RecipeBundler::Esbuild => BundlerType::Esbuild,
            RecipeBundler::Webpack => BundlerType::Webpack,
            RecipeBundler::Rollup => BundlerType::Rollup,
        }
    }

    /// The minifier the recipe's `minify` runs: each bundler's own, and
    /// terser for webpack 5 (its production minifier) and rollup (the
    /// recipe's terser plugin).
    pub fn minifier(self) -> MinifierType {
        match self {
            RecipeBundler::Bun => MinifierType::Bun,
            RecipeBundler::Esbuild => MinifierType::Esbuild,
            RecipeBundler::Webpack | RecipeBundler::Rollup => MinifierType::Terser,
        }
    }

    /// The recipe bundler a run's bundler is looked up under; None: no
    /// index slice can exist for it.
    pub fn for_bundler(bundler: BundlerType) -> Option<RecipeBundler> {
        RECIPE_BUNDLERS
            .into_iter()
            .find(|b| b.bundler_type() == bundler)
    }
}

/// The runtime a build targets: Bun's `--target`, esbuild's `--platform`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Target {
    Bun,
    Node,
}

impl Target {
    pub fn name(self) -> &'static str {
        match self {
            Target::Bun => "bun",
            Target::Node => "node",
        }
    }
}

/// The output module format (`--format`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Format {
    Cjs,
    Esm,
}

impl Format {
    pub fn name(self) -> &'static str {
        match self {
            Format::Cjs => "cjs",
            Format::Esm => "esm",
        }
    }
}

/// One build recipe = one index slice.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Recipe {
    pub bundler: RecipeBundler,
    pub target: Target,
    pub format: Format,
    pub minify: bool,
}

impl Recipe {
    /// `bun:target=bun,format=cjs,minify` — every key named, no defaults
    /// (a recipe left to a default is a recipe nobody chose).
    pub fn parse(text: &str) -> Result<Recipe, String> {
        let bad = |why: &str| {
            format!(
                "--recipe {text:?}: {why} (form: <bun|esbuild|webpack|rollup>:target=<bun|node>,format=<cjs|esm>[,minify])"
            )
        };
        let (bundler, rest) = text.split_once(':').ok_or_else(|| bad("no `:`"))?;
        let bundler = RecipeBundler::from_name(bundler).ok_or_else(|| bad("unknown bundler"))?;
        let (mut target, mut format, mut minify) = (None, None, false);
        for part in rest.split(',').map(str::trim) {
            match part.split_once('=') {
                Some(("target", "bun")) => target = Some(Target::Bun),
                Some(("target", "node")) => target = Some(Target::Node),
                Some(("format", "cjs")) => format = Some(Format::Cjs),
                Some(("format", "esm")) => format = Some(Format::Esm),
                None if part == "minify" => minify = true,
                _ => return Err(bad(&format!("unknown setting {part:?}"))),
            }
        }
        let recipe = Recipe {
            bundler,
            target: target.ok_or_else(|| bad("no target"))?,
            format: format.ok_or_else(|| bad("no format"))?,
            minify,
        };
        if recipe.bundler != RecipeBundler::Bun && recipe.target == Target::Bun {
            return Err(bad(&format!(
                "{} has no bun platform",
                recipe.bundler.name()
            )));
        }
        Ok(recipe)
    }

    /// The spelling [`Recipe::parse`] reads back.
    pub fn spelling(&self) -> String {
        format!(
            "{}:target={},format={}{}",
            self.bundler.name(),
            self.target.name(),
            self.format.name(),
            if self.minify { ",minify" } else { "" }
        )
    }

    /// The slice name: tables, shard and build directories.
    pub fn slice_name(&self) -> String {
        format!(
            "{}-{}-{}{}",
            self.bundler.name(),
            self.target.name(),
            self.format.name(),
            if self.minify { "-min" } else { "" }
        )
    }
}

/// What detection read off the bundle that picks a slice, each with its
/// reason (the toolchain record's rule: every choice says why).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DetectedRecipe {
    pub bundler: Option<RecipeBundler>,
    /// None: the bundle does not say (esbuild writes no platform marker).
    pub target: Option<Target>,
    pub target_reason: &'static str,
    /// Recorded, not matched on: the slices built so far are all one
    /// format, and a format changes only the entry's own wrapper.
    pub format: Option<Format>,
    pub minified: bool,
}

/// `// @bun …` at the very start: Bun writes it for every `--target bun`
/// build and for no other target; `@bun-cjs` marks `--format cjs`.
fn bun_banner(code: &str) -> Option<&str> {
    let first = code.trim_start_matches('\u{feff}').trim_start();
    let line = first.lines().next()?;
    let rest = line.strip_prefix("//")?.trim_start();
    (rest == "@bun" || rest.starts_with("@bun ")).then_some(rest)
}

/// The recipe a bundle was built with, as far as it says: the bundler from
/// the toolchain (resolved once), the target and format from Bun's banner.
pub fn detect_recipe(code: &str, bundler: BundlerType, minifier: MinifierType) -> DetectedRecipe {
    let bundler = RecipeBundler::for_bundler(bundler);
    let banner = bun_banner(code);
    let (target, target_reason, format) = match (bundler, banner) {
        (Some(RecipeBundler::Bun), Some(b)) => (
            Some(Target::Bun),
            "// @bun banner (Bun writes it for --target bun only)",
            Some(if b.split_whitespace().any(|t| t == "@bun-cjs") {
                Format::Cjs
            } else {
                Format::Esm
            }),
        ),
        (Some(RecipeBundler::Bun), None) => (
            Some(Target::Node),
            "no // @bun banner (Bun writes one for every --target bun build)",
            None,
        ),
        _ => (None, "the bundle does not record its platform", None),
    };
    DetectedRecipe {
        bundler,
        target,
        target_reason,
        format,
        minified: !matches!(minifier, MinifierType::None),
    }
}

impl DetectedRecipe {
    /// `bun:target=bun,format=cjs` (unknowns as `?`), for messages.
    pub fn describe(&self) -> String {
        format!(
            "{}:target={},format={}{}",
            self.bundler.map_or("?", RecipeBundler::name),
            self.target.map_or("?", Target::name),
            self.format.map_or("?", Format::name),
            if self.minified { ",minify" } else { "" }
        )
    }
}

/// The slice a detected recipe reads from `slices` (an index's recipes, in
/// manifest order) and why; Err: the index has none for it (a loud
/// WARNING in report mode, never a silent miss). Matching is on bundler
/// and target; a target the bundle does not record takes the slice the
/// index marks as that bundler's default.
pub fn choose_slice<'s>(
    slices: &'s [(Recipe, bool)],
    detected: &DetectedRecipe,
) -> Result<(&'s Recipe, &'static str), String> {
    let of_bundler = |r: &&(Recipe, bool)| Some(r.0.bundler) == detected.bundler;
    let found = match detected.target {
        Some(t) => slices
            .iter()
            .filter(of_bundler)
            .find(|r| r.0.target == t)
            .map(|r| (&r.0, detected.target_reason)),
        None => slices
            .iter()
            .filter(of_bundler)
            .find(|r| r.1)
            .map(|r| (&r.0, "the index's default slice for this bundler")),
    };
    found.ok_or_else(|| {
        let has: Vec<String> = slices.iter().map(|r| r.0.spelling()).collect();
        format!(
            "no slice for recipe {} (has: {})",
            detected.describe(),
            has.join(", ")
        )
    })
}

/// Which slices a run reads beside the one [`choose_slice`] picked (exp127,
/// `library.lookup.terser_slices`). Code minified by terser — a package's
/// own published `.min` dist, or a whole bundle re-minified — matches the
/// terser-built rows far better than its bundler's slice, and the minifier
/// verdict cannot tell terser from esbuild, so the slice choice cannot
/// rest on it.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum TerserSlices {
    /// The chosen slice alone.
    #[default]
    Off,
    /// The chosen slice plus every terser-built slice, each owner once at
    /// its smallest module.
    Union,
    /// The terser-built slices only for a print the chosen slice does not
    /// hold.
    Fallback,
    /// `Union` for a bundle whose whole-unit prints the terser-built slices
    /// ALONE hold at least this share of (nodes, over the prints either
    /// side holds; `LibraryIndex::decide_terser_slices`), else `Off`: the
    /// bundle's own prints say it was minified by terser.
    Auto { share: f64 },
}

impl TerserSlices {
    /// Every mode's spelling, in the config key's order.
    pub const NAMES: [&'static str; 4] = ["off", "union", "fallback", "auto"];

    pub fn name(self) -> &'static str {
        match self {
            TerserSlices::Off => "off",
            TerserSlices::Union => "union",
            TerserSlices::Fallback => "fallback",
            TerserSlices::Auto { .. } => "auto",
        }
    }

    /// The mode `name` spells; `share` is read by `auto` only.
    pub fn from_name(name: &str, share: f64) -> Option<TerserSlices> {
        Some(match name {
            "off" => TerserSlices::Off,
            "union" => TerserSlices::Union,
            "fallback" => TerserSlices::Fallback,
            "auto" => TerserSlices::Auto { share },
            _ => return None,
        })
    }
}

/// The index's terser-built slices other than `chosen`, in manifest order:
/// every minified slice whose bundler minifies with terser.
pub fn terser_slices<'s>(slices: &'s [(Recipe, bool)], chosen: &Recipe) -> Vec<&'s Recipe> {
    slices
        .iter()
        .map(|r| &r.0)
        .filter(|r| r.minify && r.bundler.minifier() == MinifierType::Terser && *r != chosen)
        .collect()
}
