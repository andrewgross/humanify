//! The borrowed-minified-stem answer filter (2026-10-03): the known junk
//! from the scratch-base eval is caught, and the legitimate digit-bearing
//! names the replay found stay legal.

use super::{MinifiedStems, borrowed_minified_stem, is_borrowable_stem, is_minified_echo};
use crate::rename::name_profile::NameProfile;

/// These pin the Bun profile (their replays ran on the Bun-minified
/// Claude Code corpus); `rename::name_profile`'s tests cover the others.
const BUN: NameProfile = NameProfile::Bun;

/// A program whose minified names include every stem below — and the
/// look-alikes (`b2c`, `p2s`, `x5c`, `LZ4`, `Rv4`, `V2`, `is2017`) so the
/// exemptions are tested against a program that DOES bind them.
fn stems() -> MinifiedStems {
    MinifiedStems::from_names(
        BUN,
        [
            "H6t", "uo7", "go4", "d0u", "D0u", "A0n", "da1", "_", "RHe", "b2c", "p2s", "p2c",
            "x5c", "LZ4", "X11", "Rv4", "V2", "y1", "Etl", "GT1",
        ],
    )
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
        assert!(is_borrowable_stem(BUN, name), "{name}");
    }
    for name in ["V2", "y1", "Etl", "RHe", "_", "sha256", "ipv4", "value"] {
        assert!(!is_borrowable_stem(BUN, name), "{name}");
    }
}

/// Round 2, the CASE gap (ref-scratch-0f338ffa review, 2026-10-03): the
/// model re-cases the stem it borrows — `go4Function` where the program
/// binds `Go4`, `gr9Entries` for `GR9`, `k2hResult` for `K2H`. The stem
/// comparison is case-insensitive; the disclosed stem is the answer's own
/// spelling of it.
#[test]
fn a_recased_borrowed_stem_is_refused() {
    let s = MinifiedStems::from_names(BUN, ["Go4", "Ro4", "GS7", "SS7", "GR9", "K2H"]);
    for (answer, stem) in [
        ("go4Function", "go4"),
        ("ro4Function", "ro4"),
        ("gs7Instance", "gs7"),
        ("ss7Initializer", "ss7"),
        ("gr9Entries", "gr9"),
        ("k2hResult", "k2h"),
        ("buildGo4", "Go4"),
    ] {
        assert_eq!(borrowed_minified_stem(answer, &s), Some(stem), "{answer}");
    }
}

/// Round 2, the SPLITTER gap: `S2KFunction` split as `S2` + `KFunction`,
/// so the program's `S2K` was never tested. At a digit boundary the run
/// of capitals before the next capitalised word continues the stem.
#[test]
fn a_stem_continued_by_capitals_after_its_digit_is_refused() {
    let s = MinifiedStems::from_names(BUN, ["S2K", "E2K", "Q2K", "Q2KX"]);
    for (answer, stem) in [
        ("S2KFunction", "S2K"),
        ("E2KFunction", "E2K"),
        ("Q2KFunction", "Q2K"),
        ("Q2KXHandler", "Q2KX"),
        ("handleS2KEvent", "S2K"),
        ("valueS2K", "S2K"),
    ] {
        assert_eq!(borrowed_minified_stem(answer, &s), Some(stem), "{answer}");
    }
}

/// The widened predicate keeps the known legitimate names legal — also
/// when the program binds a same-letters-other-case look-alike.
#[test]
fn the_widened_predicate_keeps_legitimate_names_legal() {
    let s = MinifiedStems::from_names(
        BUN,
        [
            "Is2", "IS2", "B2c", "P2s", "X5c", "Lz7", "Ha1", "Ut6", "s1m", "S1m", "t45",
        ],
    );
    for answer in [
        "is2017OrLater",
        "b2cLoginHosts",
        "p2sBytes",
        "x5cArray",
        "LZ77Worker",
        "Lz77Worker",
        "sha256Hash",
        "migrateSonnet1mToSonnet45",
        "MigrateSonnet1mToSonnet45",
        "getUTF8Bytes",
        "toBase64URL",
        "isHTTP2Enabled",
        "IPv4Address",
        "isIPv6",
        "Uint8Array",
        "ESC_GT1U_SEQUENCE",
    ] {
        assert_eq!(borrowed_minified_stem(answer, &s), None, "{answer}");
    }
    // The replay's case-only refusals that were real names: a trailing
    // numbered variant (`emptyFn3` beside `emptyFn2`; the program binds
    // `fn3`) and the end-to-end term (`isE2E`; the program binds `e2e`).
    // A trailing word+digits stem must match EXACTLY to be borrowed.
    let replayed = MinifiedStems::from_names(BUN, ["fn3", "e2e"]);
    for answer in ["emptyFn3", "noopFn3", "isE2E", "runE2ETests"] {
        assert_eq!(borrowed_minified_stem(answer, &replayed), None, "{answer}");
    }
    // A version number spelled with `_` (`To4_5` = "to 4.5"): the one
    // legitimate name main's predicate refused in the round-2 replay
    // (the 2.1.85 program binds `To4`).
    assert_eq!(
        borrowed_minified_stem(
            "migrateSonnet1mTo4_5",
            &MinifiedStems::from_names(BUN, ["To4"])
        ),
        None
    );
    assert_eq!(
        borrowed_minified_stem("emptyFn3", &MinifiedStems::from_names(BUN, ["Fn3"])),
        Some("Fn3"),
        "an EXACT trailing match is still borrowed (main's rule)"
    );
    // A stem bound only by a DIFFERENT program is not borrowed here.
    let other = MinifiedStems::from_names(BUN, ["Go4"]);
    assert_eq!(borrowed_minified_stem("S2KFunction", &other), None);
    assert_eq!(borrowed_minified_stem("ss7Initializer", &other), None);
}

/// Round 2, the IDENTITY ECHO: a minifier-shaped name of two or more
/// units answered with ITSELF (`{"yl":"yl"}`) is refused; a single letter
/// (a loop counter may stay), a real short word, a convention
/// placeholder and any descriptive name keep their echo.
#[test]
fn an_echoed_multi_letter_minified_name_is_refused() {
    for name in ["yl", "zf", "Ul", "kd", "lh", "$r", "z88", "Go4", "a1b"] {
        assert!(is_minified_echo(BUN, name, name), "{name}");
    }
    for name in [
        "i", "e", "x", "_", "__", "$", "fs", "id", "cb", "config", "RHe", "sha256",
    ] {
        assert!(!is_minified_echo(BUN, name, name), "{name}");
    }
    assert!(!is_minified_echo(BUN, "yl", "jobItem"), "only an echo");
}

/// 2026-10-06 (scan B1): the Bun profile's `$`-anywhere and `_`-tail
/// shape rules were what refused an answer copying a `$`/`_`-bearing
/// minified name (`initJw$`, `assignUw_`, `n$_Result` — recorded answers
/// in the latest eval logs). Those rules are gone; the PROGRAM LOOKUP
/// catches the copy instead: a `$`/`_`-bearing piece of the answer that is
/// one of this program's minified names, with a digit or a capital so it
/// cannot be a word (`$el`, `is_`), spelled EXACTLY so (case-folded, the
/// replay refused `$jQuery` for the program's `$J`).
#[test]
fn a_borrowed_dollar_or_underscore_name_is_refused_by_the_lookup() {
    let s = MinifiedStems::from_names(
        BUN,
        [
            "Jw$", "Uw_", "N$_", "$Iq", "A$", "L$", "S$", "$6", "$el", "is_", "to_", "E$", "$sc",
            "$J",
        ],
    );
    for (answer, stem) in [
        ("initJw$", "Jw$"),
        ("assignUw_", "Uw_"),
        ("setupUw_", "Uw_"),
        ("N$_Result", "N$_"),
        ("initModule_$Iq", "$Iq"),
        ("useA$", "A$"),
        ("resolveWithL$", "L$"),
        ("getS$", "S$"),
        ("init$6", "$6"),
    ] {
        assert_eq!(borrowed_minified_stem(answer, &s), Some(stem), "{answer}");
    }
    for answer in [
        "user$",
        "clicks$",
        "userClicks$",
        "worktreeState$",
        "$scope",
        "$element",
        "$elRef",
        "is_valid",
        "to_string",
        "fsPromises_",
        "case_",
        "value$",
        "$jQuery",
        // Re-cased copies stay legal (precision first; listed as misses).
        "n$_Result",
        "init$iqModule",
    ] {
        assert_eq!(borrowed_minified_stem(answer, &s), None, "{answer}");
    }
    for name in ["Jw$", "Uw_", "$Iq", "A$", "$6", "n$_9"] {
        assert!(is_borrowable_stem(BUN, name), "{name}");
    }
    for name in ["$el", "is_", "$", "_", "__", "$$", "w$i", "user$"] {
        assert!(!is_borrowable_stem(BUN, name), "{name}");
    }
}
