//! The borrowed-minified-stem answer filter (2026-10-03): the known junk
//! from the scratch-base eval is caught, and the legitimate digit-bearing
//! names the replay found stay legal.

use super::{MinifiedStems, borrowed_minified_stem, is_borrowable_stem};

/// A program whose minified names include every stem below — and the
/// look-alikes (`b2c`, `p2s`, `x5c`, `LZ4`, `Rv4`, `V2`, `is2017`) so the
/// exemptions are tested against a program that DOES bind them.
fn stems() -> MinifiedStems {
    MinifiedStems::from_names([
        "H6t", "uo7", "go4", "d0u", "D0u", "A0n", "da1", "_", "RHe", "b2c", "p2s", "p2c", "x5c",
        "LZ4", "X11", "Rv4", "V2", "y1", "Etl", "GT1",
    ])
}

#[test]
fn the_known_junk_is_refused_with_its_stem() {
    let s = stems();
    for (answer, stem) in [
        ("H6tClass", "H6t"),
        ("uo7Instance", "uo7"),
        ("go4Function", "go4"),
        ("d0uValue", "d0u"),
        ("A0nName", "A0n"),
        ("da1Regex", "da1"),
        ("envVarD0u", "D0u"),
        ("setMethodH6t", "H6t"),
        ("uo7_instance", "uo7"),
    ] {
        assert_eq!(borrowed_minified_stem(answer, &s), Some(stem), "{answer}");
    }
}

#[test]
fn legitimate_digit_names_stay_legal() {
    let s = stems();
    for answer in [
        "is2017OrLater",
        "b2cLoginHosts",
        "p2sBytes",
        "p2cIterations",
        "x5cArray",
        "LZ77Worker",
        "LZ4Encoder",
        "sha256Hash",
        "zodCidRv4Schema",
        "getX11Display",
        // Two-unit tokens are never stems (V2, y1 are mostly words).
        "isV2Enabled",
        "y1Coordinate",
        // Digitless minified names are never stems.
        "EtlParser",
        // CONSTANT_CASE answers are exempt.
        "ESC_GT1U_SEQUENCE",
        // The whole answer is not a borrowed WORD.
        "H6t",
        // A stem the program does not bind is not borrowed.
        "Zq9Thing",
        "descriptiveName",
    ] {
        assert_eq!(borrowed_minified_stem(answer, &s), None, "{answer}");
    }
}

#[test]
fn only_digit_bearing_minifier_tokens_of_three_or_more_units_are_stems() {
    for name in ["H6t", "uo7", "D0u", "A0n", "da1", "Fn8"] {
        assert!(is_borrowable_stem(name), "{name}");
    }
    for name in ["V2", "y1", "Etl", "RHe", "_", "sha256", "ipv4", "value"] {
        assert!(!is_borrowable_stem(name), "{name}");
    }
}
