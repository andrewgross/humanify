//! The split stage end to end on a regime the four oracle pairs never
//! reach (lesson 17): a runnable-emit DECLINE.

use std::collections::HashMap;

use humanify_model::js::JsValue;
use serde_json::Value;

use super::{SplitOptions, stable_split};
use crate::place::ledger::StableSplitLedger;
use crate::place::placement_dump::Regime;

/// Finding #40: a prior ledger whose names place `xa` in a.js and `yb`,
/// `zb` in b.js makes a load-time reference cycle (a.js reads b.js's `yb`
/// at load, b.js reads a.js's `xa`) — the runnable emit declines, the
/// byte-exact review tree is written, and the PERSISTED ledger keeps the
/// aliases the TS emit had assigned before it threw (the wp53 vector of
/// the same fixture records the TS's).
#[test]
fn a_declined_emit_persists_the_ts_aliases() {
    let vectors: Vec<Value> =
        serde_json::from_str(include_str!("../../../../../test/parity/wp53-cjs.json"))
            .expect("vectors");
    let v = vectors
        .iter()
        .find(|v| v["name"] == "load-time cycle declines")
        .expect("the cycle vector");
    let code = v["code"].as_str().expect("code");
    let order: Vec<String> = v["order"]
        .as_array()
        .expect("order")
        .iter()
        .map(|s| s.as_str().expect("file").to_string())
        .collect();
    let mut name_to_files: HashMap<String, Vec<String>> = HashMap::new();
    name_to_files.insert("xa".into(), vec!["a.js".into()]);
    name_to_files.insert("yb".into(), vec!["b.js".into()]);
    name_to_files.insert("zb".into(), vec!["b.js".into()]);
    for i in 0..60 {
        name_to_files.insert(format!("padFiller{i}"), vec!["pad/fill.js".into()]);
    }
    let prior = StableSplitLedger {
        version: 1,
        files: vec!["a.js".into(), "b.js".into(), "pad/fill.js".into()],
        name_to_files,
        order,
        hashes: None,
        emit_hashes: None,
        emit_names: None,
        emit_indexes: None,
        hash_version: None,
        aliases: None,
        fossil_modules: None,
    };
    let outcome = stable_split(
        code,
        SplitOptions {
            regime: Regime::Tiers,
            prior: Some(&prior),
            carry: None,
            namer: None,
            reviser: None,
            placement: Default::default(),
            align: Default::default(),
            registrar_exemption_disabled: false,
            split_pure: false,
            trail: None,
            vendor_captures: &[],
            vendor_fresh: None,
        },
    )
    .expect("the split runs");
    assert_eq!(
        outcome.declined.as_deref(),
        v["declined"].as_str(),
        "the runnable emit declines with the TS's reason"
    );
    assert!(outcome.runnable.is_none(), "the review tree is written");
    let ledger = outcome.ledger.as_object().expect("a ledger object");
    let aliases: Vec<(String, String)> = match ledger.get("aliases") {
        Some(JsValue::Object(o)) => o
            .entries()
            .iter()
            .map(|(f, a)| (f.clone(), a.as_str().unwrap_or_default().to_string()))
            .collect(),
        other => panic!("the declined ledger has no aliases: {other:?}"),
    };
    let ts: Vec<(String, String)> = v["declinedLedger"]["aliases"]
        .as_array()
        .expect("ts aliases")
        .iter()
        .map(|e| {
            (
                e[0].as_str().unwrap().to_string(),
                e[1].as_str().unwrap().to_string(),
            )
        })
        .collect();
    assert_eq!(aliases, ts);
    assert!(
        ledger.get("emitIndexes").is_none(),
        "the layout was never recorded"
    );
}

/// A wrapper bundle for the vendor-bridge tests (finding #60): statements
/// 0-1 declare the ESM init/namespace pair a vendored body reads. The
/// padding clears the wrapper's 50-binding recognition gate.
const BRIDGE_BUNDLE: &str = concat!(
    "(function (exports, require, module) {\n",
    "  var ns = {};\n",
    "  var initM = function () { ns.hi = function () { return 42; }; };\n",
    "  exports.setup = initM;\n",
    "  exports.read = function () { return (initM(), ns).hi(); };\n",
    "  var padFiller0=0,padFiller1=0,padFiller2=0,padFiller3=0,padFiller4=0,\n",
    "  padFiller5=0,padFiller6=0,padFiller7=0,padFiller8=0,padFiller9=0,\n",
    "  padFiller10=0,padFiller11=0,padFiller12=0,padFiller13=0,padFiller14=0,\n",
    "  padFiller15=0,padFiller16=0,padFiller17=0,padFiller18=0,padFiller19=0,\n",
    "  padFiller20=0,padFiller21=0,padFiller22=0,padFiller23=0,padFiller24=0,\n",
    "  padFiller25=0,padFiller26=0,padFiller27=0,padFiller28=0,padFiller29=0,\n",
    "  padFiller30=0,padFiller31=0,padFiller32=0,padFiller33=0,padFiller34=0,\n",
    "  padFiller35=0,padFiller36=0,padFiller37=0,padFiller38=0,padFiller39=0,\n",
    "  padFiller40=0,padFiller41=0,padFiller42=0,padFiller43=0,padFiller44=0,\n",
    "  padFiller45=0,padFiller46=0,padFiller47=0,padFiller48=0,padFiller49=0;\n",
    "});\n",
);

fn bridge_options<'a>(
    captures: &'a [crate::modules::vendor_names::ManifestCapture],
    fresh: Option<&'a str>,
) -> SplitOptions<'a, 'a> {
    SplitOptions {
        regime: Regime::Cluster,
        prior: None,
        carry: None,
        namer: None,
        reviser: None,
        placement: Default::default(),
        align: Default::default(),
        registrar_exemption_disabled: false,
        split_pure: false,
        trail: None,
        vendor_captures: captures,
        vendor_fresh: fresh,
    }
}

fn capture(name: &str) -> crate::modules::vendor_names::ManifestCapture {
    crate::modules::vendor_names::ManifestCapture {
        name: name.to_string(),
    }
}

/// Finding #60: the unpack's capture records — raw names a vendored body
/// READS — resolve against the FRESH text (still raw-named, aligned
/// statement-for-statement with the shipped one) and the split's placement:
/// (raw name → owner file + post-rename accessor). The owner file EXPORTS
/// the accessor whether or not any app file references the binding,
/// because the reader is a vendor body the emit cannot see. Here the
/// shipped text carries the renames (`ns` → `nsRenamed`), exactly as the
/// naming stage leaves it.
#[test]
fn vendor_captures_resolve_to_owner_files_with_forced_accessors() {
    let shipped = BRIDGE_BUNDLE
        .replace("ns", "nsRenamed")
        .replace("initM", "initMRenamed");
    let outcome = stable_split(
        &shipped,
        bridge_options(&[capture("ns"), capture("initM")], Some(BRIDGE_BUNDLE)),
    )
    .expect("the split runs");
    assert!(
        outcome.declined.is_none(),
        "{:?}",
        outcome.declined.as_deref().unwrap_or_default()
    );
    let mut bridges = outcome.vendor_bridges.clone();
    bridges.sort_by(|a, b| a.name.cmp(&b.name));
    assert_eq!(bridges.len(), 2);
    assert_eq!(bridges[0].name, "initM");
    assert_eq!(bridges[0].accessor, "initMRenamed");
    assert_eq!(bridges[1].name, "ns");
    assert_eq!(bridges[1].accessor, "nsRenamed");
    assert_eq!(
        bridges[0].file, bridges[1].file,
        "both statements share the owner file"
    );
    let owner = &bridges[0].file;
    let emitted = outcome
        .files
        .iter()
        .find(|(p, _)| p == owner)
        .expect("the owner file was emitted");
    assert_eq!(
        emitted
            .1
            .matches("Object.defineProperty(module.exports, \"initMRenamed\"")
            .count(),
        1
    );
    assert_eq!(
        emitted
            .1
            .matches("Object.defineProperty(module.exports, \"nsRenamed\"")
            .count(),
        1
    );
}

/// A capture the split cannot resolve would leave a free name in a vendor
/// file — the exact hole finding #51 closed — so it fails the split
/// loudly, never silently: an unknown raw name, a name the fresh text
/// declares twice, a fresh text that no longer aligns with the shipped
/// one, or no fresh text at all.
#[test]
fn an_unresolvable_vendor_capture_fails_the_split() {
    let err = match stable_split(
        BRIDGE_BUNDLE,
        bridge_options(&[capture("gone")], Some(BRIDGE_BUNDLE)),
    ) {
        Err(e) => e,
        Ok(_) => panic!("an unknown raw name fails"),
    };
    assert!(err.contains("gone"), "{err}");
    let err = match stable_split(BRIDGE_BUNDLE, bridge_options(&[capture("ns")], None)) {
        Err(e) => e,
        Ok(_) => panic!("captures without a fresh text fail"),
    };
    assert!(err.contains("fresh"), "{err}");
    // A fresh text whose statement list does not align with the shipped
    // one (here: one statement fewer) is refused, never guessed through.
    let misaligned = BRIDGE_BUNDLE.replace("  var ns = {};\n", "");
    let err = match stable_split(
        BRIDGE_BUNDLE,
        bridge_options(&[capture("ns")], Some(&misaligned)),
    ) {
        Err(e) => e,
        Ok(_) => panic!("a fresh text that disagrees with the shipped one fails"),
    };
    assert!(err.contains("align"), "{err}");
    // The wrapper is ONE scope: the same raw name declared by two of its
    // statements is ambiguous and refused.
    let dup = BRIDGE_BUNDLE.replace("var ns = {};", "var ns = {};\n  var ns = 1;");
    let err = match stable_split(&dup, bridge_options(&[capture("ns")], Some(&dup))) {
        Err(e) => e,
        Ok(_) => panic!("a name declared twice is refused"),
    };
    assert!(err.contains("exactly one"), "{err}");
}
