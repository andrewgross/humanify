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
