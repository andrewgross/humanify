//! The name profile: selected once from the trusted signals (the flags,
//! a definitive bundler; else the Bun fallback), and what each profile
//! treats as minifier-made.

use humanify_model::detection::{
    BundlerDetectionResult, BundlerType, BundlerVerdict, DetectionTier, MinifierType,
    MinifierVerdict,
};

use super::{
    FALLBACK_PROFILE, NAME_PROFILES, NameProfile, name_profile_named, select_name_profile,
};
use crate::detect::detect_bundle;
use crate::rename::floor::{
    MinifiedStems, borrowed_minified_stem, is_below_floor_name, is_borrowable_stem,
    is_half_mint_head, is_minified_echo, is_minifier_token, is_sweep_answer_acceptable,
};

fn verdict(
    bundler: BundlerType,
    bundler_tier: DetectionTier,
    minifier: MinifierType,
    minifier_tier: DetectionTier,
) -> BundlerDetectionResult {
    BundlerDetectionResult {
        bundler: BundlerVerdict {
            kind: bundler,
            tier: bundler_tier,
            version: None,
            conflict: None,
        },
        minifier: MinifierVerdict {
            kind: minifier,
            tier: minifier_tier,
        },
        signals: Vec::new(),
    }
}

use BundlerType as B;
use DetectionTier as T;
use MinifierType as M;

/// The `--minifier` flag selects its own profile outright (`none` → the
/// not-minified one), whatever detection said.
#[test]
fn the_minifier_flag_selects_its_own_profile() {
    let d = verdict(B::Bun, T::Definitive, M::Bun, T::Likely);
    for (minifier, profile) in [
        (M::Bun, NameProfile::Bun),
        (M::Esbuild, NameProfile::Esbuild),
        (M::Terser, NameProfile::Terser),
        (M::Swc, NameProfile::Swc),
        (M::None, NameProfile::NotMinified),
    ] {
        assert_eq!(
            select_name_profile(&d, None, Some(minifier)),
            profile,
            "{minifier:?}"
        );
    }
    // `unknown` is the no-flag sentinel.
    assert_eq!(
        select_name_profile(&d, None, Some(M::Unknown)),
        NameProfile::Bun
    );
}

/// A DETECTED minifier verdict never selects a profile, at any tier
/// (docs/plugin-spec.md P10: none of its signals is trustworthy yet) —
/// with nothing else known, the run stays on the Bun fallback.
#[test]
fn a_detected_minifier_verdict_is_not_trusted() {
    for minifier in [M::Esbuild, M::Terser, M::Swc, M::None] {
        for tier in [T::Unknown, T::Likely, T::Definitive] {
            let d = verdict(B::Unknown, T::Unknown, minifier, tier);
            assert_eq!(
                select_name_profile(&d, None, None),
                FALLBACK_PROFILE,
                "{minifier:?} {tier:?}"
            );
        }
    }
    assert_eq!(FALLBACK_PROFILE, NameProfile::Bun);
}

/// A definitive bun or esbuild bundler verdict (or the `--bundler` flag)
/// selects that bundler's renamer profile; a bundler that delegates
/// minification to a plugin, or a non-definitive verdict, falls back.
#[test]
fn a_definitive_renaming_bundler_selects_its_profile() {
    // Every Claude Code input: bun bundler (definitive), bun minifier.
    let d = verdict(B::Bun, T::Definitive, M::Bun, T::Likely);
    assert_eq!(select_name_profile(&d, None, None), NameProfile::Bun);
    let d = verdict(B::Esbuild, T::Definitive, M::Terser, T::Unknown);
    assert_eq!(select_name_profile(&d, None, None), NameProfile::Esbuild);
    let d = verdict(B::Esbuild, T::Likely, M::Unknown, T::Unknown);
    assert_eq!(select_name_profile(&d, None, None), FALLBACK_PROFILE);
    for bundler in [B::Webpack, B::Browserify, B::Rollup, B::Parcel] {
        let d = verdict(bundler, T::Definitive, M::Terser, T::Likely);
        assert_eq!(
            select_name_profile(&d, None, None),
            FALLBACK_PROFILE,
            "{bundler:?}"
        );
    }
    let d = verdict(B::Unknown, T::Unknown, M::Unknown, T::Unknown);
    assert_eq!(
        select_name_profile(&d, Some(B::Esbuild), None),
        NameProfile::Esbuild
    );
    assert_eq!(
        select_name_profile(&d, Some(B::Webpack), None),
        FALLBACK_PROFILE
    );
}

/// The committed e2e fixtures, through the real detection: the esbuild
/// bundles (definitive esbuild runtime markers) get the esbuild profile,
/// the plain fixtures (nothing detected) the Bun fallback.
#[test]
fn the_e2e_fixtures_select_through_real_detection() {
    let esbuild =
        include_str!("../../../../../test/e2e/fixtures/esbuild-bundle/build/v1.0.0/build/index.js");
    let plain =
        include_str!("../../../../../test/e2e/fixtures/disambiguation/build/v1.0.0/build/index.js");
    assert_eq!(
        select_name_profile(&detect_bundle(esbuild), None, None),
        NameProfile::Esbuild
    );
    assert_eq!(
        select_name_profile(&detect_bundle(plain), None, None),
        FALLBACK_PROFILE
    );
}

#[test]
fn every_profile_is_registered_by_name() {
    for p in NAME_PROFILES {
        assert_eq!(name_profile_named(p.name()), Ok(p));
    }
    assert!(name_profile_named("uglify").is_err());
}

/// Names every common renamer emits (measured 2026-10-03: esbuild 0.27.2,
/// terser 5.51.2 and bun 1.3.14 on one input) — minted under every
/// MINIFIER profile.
const SHARED_TOKENS: &[&str] = &["e", "t", "Q", "Qe", "lR", "O8", "u0", "$E", "_R", "e_"];

/// Real words no profile may call minted (digitless 3-character mints —
/// `eee`, `JKH`, `Xme` — cannot be told from words, so they stay out too).
const REAL_NAMES: &[&str] = &[
    "get", "ctx", "err", "fn", "id", "ok", "options", "MAX_SIZE", "sha256", "v8", "utf8",
    "getValue", "eee", "JKH", "Xme",
];

const MINIFIER_PROFILES: [NameProfile; 4] = [
    NameProfile::Bun,
    NameProfile::Esbuild,
    NameProfile::Terser,
    NameProfile::Swc,
];

#[test]
fn every_minifier_profile_reads_the_shared_renamer_shapes() {
    for p in MINIFIER_PROFILES {
        for n in SHARED_TOKENS {
            assert!(is_minifier_token(p, n), "{p:?} {n:?} is minted");
        }
        for n in REAL_NAMES {
            assert!(!is_minifier_token(p, n), "{p:?} {n:?} is a real name");
        }
    }
}

/// Every minifier profile — Bun included since 2026-10-06 (scan B1) —
/// reads the ONE measured renamer shape: a whole 3-4 character renamer
/// name with a mint head or a `$` is minted (`p5e`, `$me` are real
/// esbuild output; `qk_`, `HO$` real Bun output), and a trailing `_` is
/// judged by the name it decorates (`_k_`, `H2_` are minted, `fsPromises_`
/// is not). Longer names are the author's or the model's: the shapes only
/// the Claude Code corpus calibrated (`$` anywhere, any `_` tail, a mint
/// head at any length) are gone — they flagged RxJS `user$`, Angular
/// `$scope`, Svelte `$store`, snake_case `to_string`.
#[test]
fn every_minifier_profile_reads_three_and_four_character_tokens_only() {
    for p in MINIFIER_PROFILES {
        for n in [
            "p5e", "m5e", "a1b", "ab_", "$me", "Xm$", "q7Ab", "qk_", "HO$", "_k_", "_P_", "H2_",
            "h2_", "x_", "p5e_", "id_", "db_",
        ] {
            assert!(is_minifier_token(p, n), "{p:?} {n:?}");
        }
        for n in [
            "fsPromises_",
            "foo$bar",
            "$element",
            "user$",
            "clicks$",
            "$scope",
            "$store",
            "$http",
            "do7Function",
            "x1Coordinate",
            "is2017OrLater",
            "to_string",
            "my_var",
            "p256Key",
            "it2Command",
            "It2SetupWizard",
            "ab12c",
            "obj_",
        ] {
            assert!(!is_minifier_token(p, n), "{p:?} {n:?}");
            assert!(is_sweep_answer_acceptable(p, n), "{p:?} {n:?}");
        }
    }
}

/// What the Bun corpus's long-shape rules used to catch — the model
/// HALF-COPYING a minified name into its answer — is the program lookup's
/// job under every profile: `do7Function` is refused where the program
/// binds `do7`, and is a plain name where it does not.
#[test]
fn a_half_copied_answer_is_caught_by_the_program_lookup_not_by_shape() {
    for p in MINIFIER_PROFILES {
        let binds = MinifiedStems::from_names(p, ["do7", "H6t"]);
        assert_eq!(
            borrowed_minified_stem("do7Function", &binds),
            Some("do7"),
            "{p:?}"
        );
        assert_eq!(
            borrowed_minified_stem("H6tClass", &binds),
            Some("H6t"),
            "{p:?}"
        );
        let other = MinifiedStems::from_names(p, ["H6t"]);
        assert_eq!(borrowed_minified_stem("do7Function", &other), None, "{p:?}");
    }
}

/// `--minifier none`: no name is minifier-made (docs/plugin-spec.md P10).
#[test]
fn the_not_minified_profile_reads_nothing() {
    let n = NameProfile::NotMinified;
    for name in SHARED_TOKENS
        .iter()
        .chain(REAL_NAMES)
        .chain(&["p5e", "x_", "do7Function"])
    {
        assert!(!is_minifier_token(n, name), "{name:?}");
        assert!(!is_below_floor_name(n, name), "{name:?}");
        assert!(!is_minified_echo(n, name, name), "{name:?}");
        assert!(is_sweep_answer_acceptable(n, name), "{name:?}");
        assert!(!is_half_mint_head(n, name), "{name:?}");
    }
}

/// The minifier profiles share one rule today (the measured shapes agree,
/// module doc): the same verdict on every name.
#[test]
fn the_minifier_profiles_agree() {
    let names = SHARED_TOKENS.iter().chain(REAL_NAMES).copied().chain([
        "p5e",
        "$me",
        "ab_",
        "fsPromises_",
        "do7Function",
        "x_",
        "zz",
        "Kq$",
        "h1Title",
        "user$",
        "_k_",
    ]);
    for n in names {
        let bun = is_minifier_token(NameProfile::Bun, n);
        for p in MINIFIER_PROFILES {
            assert_eq!(is_minifier_token(p, n), bun, "{p:?} {n:?}");
        }
    }
}

/// The derived predicates follow the profile.
#[test]
fn the_derived_predicates_follow_the_profile() {
    let e = NameProfile::Esbuild;
    // An LLM answer wearing the conflict ladder's tail, a mint head or a
    // `$` at length 5+: a plain name to every sweep, Bun's included.
    for answer in ["fsPromises_", "x1Coordinate", "foo$bar"] {
        for p in MINIFIER_PROFILES {
            assert!(is_sweep_answer_acceptable(p, answer), "{p:?} {answer}");
        }
    }
    // `p5e` — an esbuild 3-character mint: below the floor and an echo.
    assert!(is_below_floor_name(e, "p5e"));
    assert!(is_minified_echo(e, "p5e", "p5e"));
    for p in MINIFIER_PROFILES {
        assert!(is_minified_echo(p, "Qe", "Qe"), "{p:?}");
    }
    // The half-mint head reads the HEAD's shape, under every profile.
    for p in MINIFIER_PROFILES {
        assert!(is_half_mint_head(p, "do7Function"), "{p:?}");
        assert!(is_half_mint_head(p, "T7Class"), "{p:?}");
        assert!(!is_half_mint_head(p, "v8Engine"), "{p:?}");
        assert!(!is_half_mint_head(p, "h1Title"), "{p:?}");
    }
}

/// The borrowed-stem check's PROGRAM LOOKUP is always on: under every
/// profile, a word of the answer that is one of this program's
/// 3-4-character digit-bearing minified names is refused.
#[test]
fn the_borrowed_stem_lookup_runs_under_every_profile() {
    for p in NAME_PROFILES {
        let stems = MinifiedStems::from_names(p, ["p5e", "H6t"]);
        assert_eq!(
            borrowed_minified_stem("p5eHandler", &stems),
            Some("p5e"),
            "{p:?}"
        );
        assert_eq!(
            borrowed_minified_stem("setH6t", &stems),
            Some("H6t"),
            "{p:?}"
        );
        assert_eq!(
            borrowed_minified_stem("is2017OrLater", &stems),
            None,
            "{p:?}"
        );
        assert!(is_borrowable_stem(p, "p5e"), "{p:?}");
    }
    // The guard is the renamer alphabet's 3-4 characters under every
    // profile: a program binding `is2017` (a real name, not a mint) lends
    // no stem.
    for p in NAME_PROFILES {
        let stems = MinifiedStems::from_names(p, ["is2017"]);
        assert!(!is_borrowable_stem(p, "is2017"), "{p:?}");
        assert_eq!(
            borrowed_minified_stem("is2017OrLater", &stems),
            None,
            "{p:?}"
        );
    }
}
