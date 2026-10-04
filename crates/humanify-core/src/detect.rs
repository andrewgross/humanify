//! Stage 1: bundle detection (TS: src/detection/detect.ts, WPB.1) — which
//! bundler wrapped the input and which minifier produced it, from string
//! signals in the first 16K UTF-16 code units. A pipeline stage, not a CLI
//! concern (02 §2): the CLI's `--bundler` / `--minifier` overrides are
//! applied on top of this verdict by the caller.
//!
//! Gate: `humanify detect <input>` prints `serde_json::to_string` of the
//! verdict, byte-compared with the TS `JSON.stringify(detectBundle(code))`
//! (test/parity/wpb1-detect-probe.ts). Since 2026-10-04 the bundler
//! verdict departs from the TS on purpose: it is decided by signal
//! STRENGTH, not by detector order, and carries a `conflict` when the
//! signals name more than one bundler (toolchain review R3/R19). On input
//! whose signals agree — every Claude Code release walked — the JSON is
//! byte-identical to before.

pub mod js_text;
pub mod signals;

use humanify_model::detection::{
    BundlerConflict, BundlerDetectionResult, BundlerType, BundlerVerdict, ConflictCandidate,
    ConflictResolution, DetectionSignal, DetectionTier, MinifierType, MinifierVerdict,
};

#[cfg(doc)]
use humanify_model::detection::SignalStrength;

use self::js_text::js_prefix;
use self::signals::{
    detect_browserify, detect_bun_bundler, detect_esbuild, detect_minifier, detect_parcel,
    detect_webpack,
};

/// TS `SCAN_LIMIT`: 16K, in UTF-16 code units (`code.slice(0, SCAN_LIMIT)`).
const SCAN_LIMIT: usize = 16 * 1024;

/// TS `BUNDLER_DETECTORS`. The order is the order of the `signals` list
/// only; which signal wins is decided by strength ([`pick_bundler`]).
const BUNDLER_DETECTORS: [fn(&str) -> Vec<DetectionSignal>; 5] = [
    detect_webpack,
    detect_browserify,
    detect_esbuild,
    detect_parcel,
    detect_bun_bundler,
];

/// TS `detectBundle`.
pub fn detect_bundle(code: &str) -> BundlerDetectionResult {
    let slice = js_prefix(code, SCAN_LIMIT);
    let mut signals: Vec<DetectionSignal> =
        BUNDLER_DETECTORS.iter().flat_map(|d| d(slice)).collect();
    signals.extend(detect_minifier(slice));
    BundlerDetectionResult {
        bundler: pick_bundler(&signals),
        minifier: pick_minifier(&signals),
        signals,
    }
}

/// Every bundler the signals name, each at its STRONGEST signal, strongest
/// first. Among equal strengths the first-listed signal is kept for the
/// pattern and the order — display only: no decision reads that order.
fn candidates(signals: &[DetectionSignal]) -> Vec<ConflictCandidate> {
    let mut best: Vec<ConflictCandidate> = Vec::new();
    for s in signals {
        let (Some(bundler), Some(strength)) = (s.bundler, s.strength) else {
            continue;
        };
        match best.iter_mut().find(|c| c.bundler == bundler) {
            Some(c) if strength > c.strength => {
                c.strength = strength;
                c.pattern = s.pattern.clone();
            }
            Some(_) => {}
            None => best.push(ConflictCandidate {
                bundler,
                pattern: s.pattern.clone(),
                strength,
            }),
        }
    }
    // Stable: equal strengths keep their listed order.
    best.sort_by_key(|c| std::cmp::Reverse(c.strength));
    best
}

/// The bundler verdict, by STRENGTH (the ranking's rules are on
/// [`SignalStrength`]; toolchain review R3/R19): the bundler whose
/// strongest signal outranks every other bundler's, at that signal's tier
/// (`definitive`, or `likely` for a weak token alone). Two bundlers at the
/// strongest rank is a tie: `unknown`. Whenever the signals name more than
/// one bundler the verdict carries the conflict — every candidate and how
/// it was settled — so the choice is never silent and never list order.
pub(crate) fn pick_bundler(signals: &[DetectionSignal]) -> BundlerVerdict {
    let candidates = candidates(signals);
    let tied = candidates.len() > 1 && candidates[0].strength == candidates[1].strength;
    let winner = candidates.first().filter(|_| !tied);
    let (kind, tier) = match winner {
        Some(c) => (c.bundler, c.strength.tier()),
        None => (BundlerType::Unknown, DetectionTier::Unknown),
    };
    let resolution = if tied {
        ConflictResolution::Tie
    } else {
        ConflictResolution::Strength
    };
    let conflict = (candidates.len() > 1).then_some(BundlerConflict {
        resolution,
        candidates,
    });
    BundlerVerdict {
        kind,
        tier,
        version: None,
        conflict,
    }
}

/// The highest-tier minifier signal; ties keep the EARLIEST (the TS
/// `reduce` replaces the accumulator only on a strictly greater rank).
fn pick_minifier(signals: &[DetectionSignal]) -> MinifierVerdict {
    let best = signals
        .iter()
        .filter_map(|s| s.minifier.map(|m| (m, s.tier)))
        .reduce(|a, b| if b.1 > a.1 { b } else { a });
    match best {
        Some((kind, tier)) => MinifierVerdict { kind, tier },
        None => MinifierVerdict {
            kind: MinifierType::Unknown,
            tier: DetectionTier::Unknown,
        },
    }
}
