//! The minifier NAME PROFILE: which names look minifier-made, for the
//! minifier that actually produced this input (Andrew, 2026-10-03: "the
//! token checking ... should be loaded dynamically based on the detected
//! minifier, so we don't run bun detection on esbuild etc. The generic
//! ones are fine to run always").
//!
//! Selected ONCE per run from detection ([`select_name_profile`], the
//! unpack registry's style: a registry in fixed order, each profile's
//! `supports` read off the detection verdict) and handed to every consumer
//! of the name-shape predicates in `rename::floor` — nothing below the
//! selection names a minifier.
//!
//! What each renamer emits, measured 2026-10-03 on the same input
//! (typescript.js, ~7,600 bindings) minified by esbuild 0.27.2, terser
//! 5.51.2 and bun 1.3.14: all three draw from the same 64-character
//! alphabet — first character `[A-Za-z_$]`, later ones `[A-Za-z0-9_$]` —
//! and lengthen 1 → 2 → 3 characters only as a scope runs out of shorter
//! names (54 one-character names, then ~3,400 two-character ones; four
//! characters only past ~220,000). A digit is never first. The orders
//! differ (terser `ee te ne`, esbuild/bun frequency-shuffled `lR O8`), the
//! SHAPES do not. Bun's renamer is a port of esbuild's.
//!
//! - [`NameProfile::Bun`], [`NameProfile::Esbuild`],
//!   [`NameProfile::Terser`], [`NameProfile::Swc`] — the renamer-alphabet
//!   rules: a 1-2 character non-word, or a WHOLE name of 3-4 characters in
//!   the renamer alphabet that carries a mint head or a `$` (`p5e`, `u0`,
//!   `$me`, `ab_`, Bun's `qk_`, `HO$`); otherwise a trailing `_` is judged
//!   by the name it decorates (Bun's `_k_`, `H2_`). Digitless
//!   three-character names (`eee`, `Xme`, `JKH`) cannot be told from words
//!   and stay out (precision first). swc's mangler is terser's base-54
//!   scheme. The four share one rule set because the measured shapes
//!   agree; they are separate entries so a measured difference has
//!   somewhere to land. Replayed over every binding of the eight Claude
//!   Code inputs, the shared rule and Bun's old one disagree on ONE name,
//!   `___` (a convention placeholder the sweep never asks about).
//!
//!   UNTIL 2026-10-06 the Bun profile added three rules calibrated on the
//!   Claude Code corpus (scan B1): `$` anywhere, any trailing `_`, and a
//!   mint head (`^[A-Za-z]{1,2}[0-9_]`) at ANY length — so the model's
//!   half-copied answers (`do7Function`) counted as minted. Bun's renamer
//!   emits none of those long shapes, and as the fallback for every unsure
//!   input they misfired on real names in other apps (RxJS `user$`,
//!   Angular `$scope`, Svelte `$store`, snake_case `to_string`). What they
//!   were for — the answer copies a minified name — is the program
//!   lookup's job (`floor::borrowed_minified_stem`, which reads THIS
//!   program's minified bindings, `$`/`_`-bearing ones included).
//! - [`NameProfile::NotMinified`] — a declared-unminified input
//!   (`--minifier none`): no name is minifier-made (docs/plugin-spec.md
//!   P10: "a not-minified verdict picks a profile that counts nothing as
//!   minted"). Every name is the author's.
//!
//! WHICH SIGNALS ARE TRUSTED (docs/plugin-spec.md P10, 2026-10-03): only
//! a confident verdict switches away from Bun, and an unsure one stays on
//! Bun until minifier detection is fixed — since 2026-10-06 that fallback
//! is the generic renamer rule, not a Claude-Code-tuned one. Trusted:
//! the `--minifier` and `--bundler` flags, and a DEFINITIVE bundler
//! detection (bun's and esbuild's runtime markers). NOT trusted, so never
//! read here: every minifier detection signal. Measured 2026-10-03 —
//! terser's signals (`void 0`, `!0`) fire on any minified file (detection
//! calls esbuild- and bun-minified typescript.js "terser"; the
//! terser-minified copy reads "unknown"); the "esbuild minifier" signal is
//! esbuild's `// path.js` banner, which only an UNMINIFIED bundle keeps;
//! the "bun minifier" signal counts `$Ab`-shaped names, which `$scope` /
//! `$http` code and esbuild/terser output trip as well.
//!
//! The profiles nest: NotMinified reads nothing, the four minifier
//! profiles read the same tokens.
//!
//! Profile-INDEPENDENT (the always-on generic checks, Andrew's "fine to
//! run always"): the convention placeholders (`_`, `$` —
//! `floor::is_convention_carveout`), single letters as an acceptable
//! answer, CONSTANT_CASE, the reconcile's wordless-shape metric, and the
//! borrowed-stem check's PROGRAM LOOKUP (a piece of the answer that is
//! literally one of this program's minified binding names). That lookup's
//! shape filter is a precision guard, not the evidence; it is the
//! renamer-alphabet guard under every profile, NotMinified included
//! (`floor::is_borrowable_stem`).

use humanify_model::detection::{BundlerDetectionResult, BundlerType, DetectionTier, MinifierType};

/// One minifier's name-shape knowledge (see the module doc).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NameProfile {
    Bun,
    Esbuild,
    Terser,
    Swc,
    NotMinified,
}

/// Registry order: the minifier-specific profiles, then the not-minified
/// one.
pub const NAME_PROFILES: [NameProfile; 5] = [
    NameProfile::Bun,
    NameProfile::Esbuild,
    NameProfile::Terser,
    NameProfile::Swc,
    NameProfile::NotMinified,
];

impl NameProfile {
    /// The profile's registered name (recorded in the run's selection).
    pub fn name(self) -> &'static str {
        match self {
            NameProfile::Bun => "bun",
            NameProfile::Esbuild => "esbuild",
            NameProfile::Terser => "terser",
            NameProfile::Swc => "swc",
            NameProfile::NotMinified => "none",
        }
    }

    /// The profile for a minifier verdict, if one is specific to it
    /// (`none` and `unknown` have none).
    fn of_minifier(kind: MinifierType) -> Option<NameProfile> {
        match kind {
            MinifierType::Bun => Some(NameProfile::Bun),
            MinifierType::Esbuild => Some(NameProfile::Esbuild),
            MinifierType::Terser => Some(NameProfile::Terser),
            MinifierType::Swc => Some(NameProfile::Swc),
            MinifierType::None | MinifierType::Unknown => None,
        }
    }

    /// The profile a BUNDLER implies: only the bundlers that minify with
    /// their OWN renamer (bun, esbuild). webpack/parcel/rollup/browserify
    /// delegate minification to a plugin whose choice the bundle does not
    /// reveal — no guess.
    pub(crate) fn of_bundler(kind: BundlerType) -> Option<NameProfile> {
        match kind {
            BundlerType::Bun => Some(NameProfile::Bun),
            BundlerType::Esbuild => Some(NameProfile::Esbuild),
            _ => None,
        }
    }
}

/// The profile when nothing confident is known: Bun (docs/plugin-spec.md
/// P10 — an unsure verdict must not silently move any output until
/// minifier detection can be trusted). Since 2026-10-06 the Bun profile
/// IS the generic renamer rule, so the fallback assumes nothing about the
/// app.
pub const FALLBACK_PROFILE: NameProfile = NameProfile::Bun;

/// `select_name_profile`: the ONE place this run's name profile is
/// decided, from the trusted signals only (module doc), in order:
///
/// 1. a `--minifier` flag (other than `unknown`) decides outright —
///    `none` selects NotMinified;
/// 2. the bundler — a `--bundler` flag, else a DEFINITIVE detection —
///    when it minifies with its own renamer (bun → Bun, esbuild →
///    Esbuild; Bun's own minifier is identified by the bun bundler);
/// 3. otherwise [`FALLBACK_PROFILE`].
///
/// The detected MINIFIER verdict is deliberately not read (module doc:
/// none of its signals is trustworthy yet).
pub fn select_name_profile(
    detection: &BundlerDetectionResult,
    bundler_override: Option<BundlerType>,
    minifier_override: Option<MinifierType>,
) -> NameProfile {
    if let Some(kind) = minifier_override.filter(|m| *m != MinifierType::Unknown) {
        return NameProfile::of_minifier(kind).unwrap_or(NameProfile::NotMinified);
    }
    let bundler =
        bundler_override
            .filter(|b| *b != BundlerType::Unknown)
            .or((detection.bundler.tier == DetectionTier::Definitive)
                .then_some(detection.bundler.kind));
    bundler
        .and_then(NameProfile::of_bundler)
        .unwrap_or(FALLBACK_PROFILE)
}

/// `selectNameProfile(name)`: the profile registered under `name` (the
/// pipeline config carries the selection by name, as it does the unpack
/// adapter's); an unknown name is an error.
pub fn name_profile_named(name: &str) -> Result<NameProfile, String> {
    NAME_PROFILES
        .into_iter()
        .find(|p| p.name() == name)
        .ok_or_else(|| format!("No name profile named \"{name}\""))
}

#[cfg(test)]
mod name_profile_test;
