//! Stage 1: bundle detection (TS: src/detection/detect.ts, WPB.1) — which
//! bundler wrapped the input and which minifier produced it, from string
//! signals in the first 16K UTF-16 code units. A pipeline stage, not a CLI
//! concern (02 §2): the CLI's `--bundler` / `--minifier` overrides are
//! applied on top of this verdict by the caller.
//!
//! Gate: `humanify detect <input>` prints `serde_json::to_string` of the
//! verdict, byte-compared with the TS `JSON.stringify(detectBundle(code))`
//! (test/parity/wpb1-detect-probe.ts).

pub mod js_text;
pub mod signals;

use humanify_model::detection::{
    BundlerDetectionResult, BundlerType, BundlerVerdict, DetectionSignal, DetectionTier,
    MinifierType, MinifierVerdict,
};

use self::js_text::js_prefix;
use self::signals::{
    detect_browserify, detect_bun_bundler, detect_esbuild, detect_minifier, detect_parcel,
    detect_webpack,
};

/// TS `SCAN_LIMIT`: 16K, in UTF-16 code units (`code.slice(0, SCAN_LIMIT)`).
const SCAN_LIMIT: usize = 16 * 1024;

/// TS `BUNDLER_DETECTORS`, in order (the order decides which definitive
/// signal wins when several fire).
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

/// The FIRST definitive bundler signal, else unknown.
fn pick_bundler(signals: &[DetectionSignal]) -> BundlerVerdict {
    let first = signals
        .iter()
        .find(|s| s.bundler.is_some() && s.tier == DetectionTier::Definitive)
        .and_then(|s| s.bundler);
    match first {
        Some(kind) => BundlerVerdict {
            kind,
            tier: DetectionTier::Definitive,
            version: None,
        },
        None => BundlerVerdict {
            kind: BundlerType::Unknown,
            tier: DetectionTier::Unknown,
            version: None,
        },
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
