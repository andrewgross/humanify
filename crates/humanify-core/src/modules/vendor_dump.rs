//! The WPB.2-adjacent gate's Rust-side dump: rebuild the Bun vendor naming,
//! the written `_bun-modules.json` manifest, and the structuralSignature
//! partition family from a TS dump's MINIFIED text and the prior release's
//! manifest. TS originals: `src/unpack/adapters/bun.ts` (the manifest
//! write), `src/dump/write.ts:readVendorSignatureFamily` (the family).
//! Migration scaffolding — deleted at phase 6 with the TS core (02 §9).
//!
//! ## What the gate needs
//!
//! - `ts_dump` — a TS dump dir (`meta.json` + `text/minified.js`): the
//!   classification the naming cascade runs on is the UNPACK-site one, on
//!   the MINIFIED text (the fresh text's marker scan misses — the
//!   beautifier line-splits `{exports:{}}`).
//! - `prior_manifest` — the PRIOR release's written
//!   `vendor/_bun-modules.json` (what `--prior-version` resolves through
//!   `loadPriorVendorNames` / `loadPriorManifestFactories`). The prior
//!   trees are the humanified output trees (e.g.
//!   `/work/exp050-cold/<v>-rebased/vendor/_bun-modules.json`). Absent or
//!   unparseable → no carry-over and bundle order, exactly like the TS.
//! - the LLM boundary — `--llm-cache <dir>` (+ the model params the oracle
//!   run used, from its meta.json `flags`) wires the cache-replaying
//!   namer; without it the LLM pass is SKIPPED, as in the TS (the pass only
//!   runs when `options?.vendorNamer`), and parity then holds only where
//!   every factory resolved without the LLM.
//!
//! ## The comparison artifacts
//!
//! - `manifest.json` — the rebuilt manifest, in the TS-written shape (same
//!   field order, 2-space pretty + trailing newline, prior-release entry
//!   order). Compare against the TS run's own written
//!   `vendor/_bun-modules.json` (`--expect-manifest <path>` diffs it and
//!   fails on any divergence).
//! - `partitions.json` — the structuralSignature family, members keyed by
//!   the emitted vendor FILE PATH with zero spans (07 §1's tree-relative-
//!   path key space), as `readVendorSignatureFamily` builds it.
//!
//! KNOWN TS-WRITER DEFECT the gate documents rather than copies: the TS
//! `writePartitions` pipes every family member through
//! `anchors.convert(familyAnchor(family), …)`, which OVERWRITES the member's
//! `text` with the anchor label — so the TS's written structuralSignature
//! family collapses every member to `{text:"fresh", start:0, end:0}` and
//! the file-path identity the code's own comment claims ("the member
//! identity is the emitted vendor FILE PATH itself") never reaches disk.
//! The dump's `compare --sections partitions` therefore compares that
//! family as a single collapsed row — inert. This gate writes the FILE-PATH
//! keys the design intends (the discriminating artifact); `--collapsed-
//! member-keys` emits the TS-as-written degenerate shape when a compare
//! against an existing TS dump's partitions.json is wanted (it can only
//! prove family presence, never member identity).

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

use oxc_allocator::Allocator;
use serde_json::{Value, json};

use crate::hash::serialize::SymbolTables;
use crate::ingest::Ingest;
use crate::modules::vendor_names::{
    BunModulesManifest, ManifestEntry, VendorNamer, annotate_hash_ordinals, choose_file_names,
    load_prior_manifest_factories, load_prior_vendor_names, name_fallback_factories_with_llm,
    order_by_prior_manifest,
};
use crate::modules::wrapper::find_wrapper_function;
use crate::modules::{classify_bun_modules, name_cjs_factories};
use crate::unpack::bun::IdentifierPlanner;

/// The vendor folder (split/layout.ts VENDOR_DIR).
const VENDOR_DIR: &str = "vendor";

/// Everything the gate runs with (the TS `UnpackOptions` slice + the dump
/// inputs).
pub struct VendorNamesGate<'n> {
    /// The LLM namer — None skips the LLM pass (the TS does the same).
    pub namer: Option<&'n mut dyn VendorNamer>,
    /// Write the TS-as-written degenerate member keys instead of the file
    /// paths (see the module header).
    pub collapsed_member_keys: bool,
    /// The TS run's own written `vendor/_bun-modules.json` to diff the
    /// rebuilt manifest against (`--expect-manifest`).
    pub expect_manifest: Option<String>,
}

/// What the gate produced — printed as the gate's summary.
#[derive(Debug)]
pub struct VendorNamesReport {
    pub factories: usize,
    pub name_sources: HashMap<String, usize>,
    pub family_members: usize,
    /// Divergences against `--expect-manifest`, when given.
    pub manifest_divergences: Vec<String>,
}

/// Run the gate: rebuild the manifest + the structuralSignature family from
/// the TS dump's minified text, write `meta.json` + `manifest.json` +
/// `partitions.json` into `out_dir`.
pub fn dump_vendor_names(
    ts_dump_dir: &Path,
    prior_manifest_path: Option<&Path>,
    out_dir: &Path,
    mut gate: VendorNamesGate<'_>,
) -> Result<VendorNamesReport, String> {
    let meta_text =
        fs::read_to_string(ts_dump_dir.join("meta.json")).map_err(|e| format!("meta.json: {e}"))?;
    let meta: Value = serde_json::from_str(&meta_text).map_err(|e| format!("meta: {e}"))?;
    let minified = fs::read_to_string(ts_dump_dir.join("text").join("minified.js"))
        .map_err(|e| format!("minified text: {e}"))?;

    // The prior manifest: absent/unparseable → no carry-over and bundle
    // order (the TS's loadPriorVendorNames / loadPriorManifestFactories
    // contract).
    let prior_text = prior_manifest_path
        .map(fs::read_to_string)
        .transpose()
        .map_err(|e| format!("prior manifest: {e}"))?;
    let prior_names = prior_text.as_deref().and_then(load_prior_vendor_names);
    let prior_factories = prior_text
        .as_deref()
        .and_then(load_prior_manifest_factories);

    let allocator = Allocator::default();
    let ingest = Ingest::parse(&allocator, &minified, "input.js");
    if !ingest.errors.is_empty() {
        // The TS adapter would fall back to the regex extractor here; the
        // gate refuses instead — the regex path is the parse-failure floor
        // and the four-pair gate inputs all parse.
        return Err(format!(
            "oxc on the minified text: {} diagnostic(s)",
            ingest.errors.len()
        ));
    }
    let wrapper = find_wrapper_function(ingest.program, ingest.semantic());
    let tables = SymbolTables::build(ingest.semantic());
    let mut classification = classify_bun_modules(
        &minified,
        ingest.program,
        ingest.semantic(),
        wrapper.as_ref().map(|w| w.body_span),
        &tables,
    )
    .ok_or("no Bun CJS factory helper in the minified text — not a Bun bundle")?;

    // The deterministic cascade (banner → url → carry-over → fallback).
    name_cjs_factories(&mut classification, &minified, prior_names.as_ref());

    // Post-cascade LLM pass over the hash-named leftovers only.
    if let Some(namer) = gate.namer.as_deref_mut() {
        name_fallback_factories_with_llm(&mut classification.factories, &minified, namer);
    }

    // File names (vendor/<fileName>.js), in bundle order.
    let lookups = choose_file_names(&classification.factories);

    // The stable-identifier rewrite plan (runtimeIdentifier): needs the
    // live scopes — the declaration is still present, the only moment the
    // references are resolvable.
    let planner = IdentifierPlanner::build(&ingest);
    let mut used_identifiers: HashSet<String> = HashSet::new();
    let entries: Vec<ManifestEntry> = classification
        .factories
        .iter()
        .zip(lookups)
        .map(|(record, lookup)| {
            let runtime_identifier =
                planner
                    .plan(&ingest, record, &used_identifiers)
                    .map(|(identifier, _)| {
                        used_identifiers.insert(identifier.clone());
                        identifier
                    });
            ManifestEntry {
                file_name: format!("{VENDOR_DIR}/{}.js", lookup.file_name),
                name: lookup.name,
                name_source: lookup.name_source.as_str(),
                structural_hash: lookup.structural_hash,
                runtime_identifier,
                hash_ordinal: None,
                banner_package: record.banner_package.clone(),
                banner_version: record.banner_version.clone(),
            }
        })
        .collect();

    // Ordinals are stamped in BUNDLE order (the tie-break's order), BEFORE
    // the entries are reordered to follow the prior release.
    let manifest = BunModulesManifest {
        adapter: "bun",
        hash_version: crate::modules::FACTORY_HASH_VERSION,
        runtime_file: Some("runtime.js".to_string()),
        factories: order_by_prior_manifest(
            annotate_hash_ordinals(entries),
            prior_factories.as_deref(),
        ),
    };

    // The structuralSignature family, members keyed by the emitted vendor
    // file path with zero spans — sorted by spanKeyOrder (text, start, end).
    let mut members: Vec<Value> = manifest
        .factories
        .iter()
        .filter(|f| !f.structural_hash.is_empty())
        .map(|f| {
            let member = if gate.collapsed_member_keys {
                json!({"text": "fresh", "start": 0, "end": 0})
            } else {
                json!({"text": f.file_name, "start": 0, "end": 0})
            };
            json!({"member": member, "hash": f.structural_hash})
        })
        .collect();
    members.sort_by(|a, b| {
        let key = |v: &Value| {
            (
                v["member"]["text"].as_str().unwrap_or("").to_string(),
                v["member"]["start"].as_i64().unwrap_or(0),
                v["member"]["end"].as_i64().unwrap_or(0),
            )
        };
        key(a).cmp(&key(b))
    });
    let family_members = members.len();

    fs::create_dir_all(out_dir).map_err(|e| format!("mkdir: {e}"))?;
    fs::write(
        out_dir.join("meta.json"),
        serde_json::to_string(&meta).unwrap(),
    )
    .map_err(|e| format!("write meta: {e}"))?;
    // The TS writes the manifest `JSON.stringify(manifest, null, 2)` + "\n".
    let manifest_json = serde_json::to_string_pretty(&manifest).unwrap();
    fs::write(out_dir.join("manifest.json"), format!("{manifest_json}\n"))
        .map_err(|e| format!("write manifest: {e}"))?;
    fs::write(
        out_dir.join("partitions.json"),
        serde_json::to_string_pretty(&json!({
            "schemaVersion": 1,
            "families": if members.is_empty() {
                vec![]
            } else {
                vec![json!({"family": "structuralSignature", "members": members})]
            },
        }))
        .unwrap(),
    )
    .map_err(|e| format!("write partitions: {e}"))?;

    // Optional self-check: diff against the TS run's own written manifest.
    let mut divergences = Vec::new();
    if let Some(expect_path) = gate.expect_manifest_path() {
        divergences = diff_against_expected(expect_path, &manifest)?;
    }

    let mut name_sources: HashMap<String, usize> = HashMap::new();
    for f in &manifest.factories {
        *name_sources.entry(f.name_source.to_string()).or_insert(0) += 1;
    }
    Ok(VendorNamesReport {
        factories: manifest.factories.len(),
        name_sources,
        family_members,
        manifest_divergences: divergences,
    })
}

impl VendorNamesGate<'_> {
    fn expect_manifest_path(&self) -> Option<&Path> {
        self.expect_manifest.as_deref().map(Path::new)
    }
}

/// Diff the rebuilt manifest against the TS run's own written
/// `vendor/_bun-modules.json`, field by field (the TS-written JSON carries
/// the same field names). Returns one line per divergence.
fn diff_against_expected(
    expect_path: &Path,
    rebuilt: &BunModulesManifest,
) -> Result<Vec<String>, String> {
    let text = fs::read_to_string(expect_path).map_err(|e| format!("expect manifest: {e}"))?;
    let expected: Value =
        serde_json::from_str(&text).map_err(|e| format!("expect manifest parse: {e}"))?;
    let mut out = Vec::new();
    let expected_factories = expected
        .get("factories")
        .and_then(|f| f.as_array())
        .cloned()
        .unwrap_or_default();
    if expected_factories.len() != rebuilt.factories.len() {
        out.push(format!(
            "factories: expected {}, rebuilt {}",
            expected_factories.len(),
            rebuilt.factories.len()
        ));
    }
    for (i, (expected_entry, rebuilt_entry)) in expected_factories
        .iter()
        .zip(rebuilt.factories.iter())
        .enumerate()
    {
        let check = |out: &mut Vec<String>,
                     field: &str,
                     expected_value: Option<&str>,
                     rebuilt_value: &str| {
            let expected_value = expected_value.unwrap_or("");
            if expected_value != rebuilt_value {
                out.push(format!(
                    "factories[{i}].{field}: expected {expected_value:?}, rebuilt {rebuilt_value:?}"
                ));
            }
        };
        check(
            &mut out,
            "fileName",
            expected_entry.get("fileName").and_then(|v| v.as_str()),
            &rebuilt_entry.file_name,
        );
        check(
            &mut out,
            "name",
            expected_entry.get("name").and_then(|v| v.as_str()),
            &rebuilt_entry.name,
        );
        check(
            &mut out,
            "nameSource",
            expected_entry.get("nameSource").and_then(|v| v.as_str()),
            rebuilt_entry.name_source,
        );
        check(
            &mut out,
            "structuralHash",
            expected_entry
                .get("structuralHash")
                .and_then(|v| v.as_str()),
            &rebuilt_entry.structural_hash,
        );
        check(
            &mut out,
            "runtimeIdentifier",
            expected_entry
                .get("runtimeIdentifier")
                .and_then(|v| v.as_str()),
            rebuilt_entry.runtime_identifier.as_deref().unwrap_or(""),
        );
        let expected_ordinal = expected_entry
            .get("hashOrdinal")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize);
        if expected_ordinal != rebuilt_entry.hash_ordinal {
            out.push(format!(
                "factories[{i}].hashOrdinal: expected {:?}, rebuilt {:?}",
                expected_ordinal, rebuilt_entry.hash_ordinal
            ));
        }
        check(
            &mut out,
            "bannerPackage",
            expected_entry.get("bannerPackage").and_then(|v| v.as_str()),
            rebuilt_entry.banner_package.as_deref().unwrap_or(""),
        );
        check(
            &mut out,
            "bannerVersion",
            expected_entry.get("bannerVersion").and_then(|v| v.as_str()),
            rebuilt_entry.banner_version.as_deref().unwrap_or(""),
        );
    }
    Ok(out)
}

#[cfg(test)]
mod gate_tests {
    use super::*;
    use serde_json::Value;
    use std::path::PathBuf;

    const FIXTURE: &str = concat!(
        "var x=(I,A)=>()=>(A||I((A = {exports:{}}).exports, A), A.exports);\n",
        "var depOne=x((exports,module)=>{ module.exports=function one(a){return a+1}; });\n",
        "var depTwo=x((exports,module)=>{ module.exports=function two(a,b,c){return a*b*c}; });\n",
        "var shimOne=x((exports,module)=>{ module.exports=depOne(); });\n",
        "var shimTwo=x((exports,module)=>{ module.exports=depTwo(); });\n",
        "var main=shimOne();"
    );

    fn temp_root(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("humanify-vendor-gate-{tag}-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_ts_dump(dir: &Path) {
        std::fs::create_dir_all(dir.join("text")).unwrap();
        std::fs::write(dir.join("meta.json"), r#"{"commit":"test"}"#).unwrap();
        std::fs::write(dir.join("text").join("minified.js"), FIXTURE).unwrap();
    }

    fn run(
        ts_dump: &Path,
        out: &Path,
        prior: Option<&Path>,
        expect: Option<&Path>,
    ) -> VendorNamesReport {
        dump_vendor_names(
            ts_dump,
            prior,
            out,
            VendorNamesGate {
                namer: None,
                collapsed_member_keys: false,
                expect_manifest: expect.map(|p| p.to_string_lossy().into_owned()),
            },
        )
        .expect("the gate should run")
    }

    fn assert_manifest_written(manifest_path: &Path, factories_expected: usize) {
        let manifest: Value =
            serde_json::from_str(&std::fs::read_to_string(manifest_path).unwrap()).unwrap();
        assert_eq!(manifest["adapter"], "bun");
        assert_eq!(manifest["runtimeFile"], "runtime.js");
        let factories = manifest["factories"].as_array().unwrap();
        assert_eq!(factories.len(), factories_expected);
        for entry in factories {
            let file_name = entry["fileName"].as_str().unwrap();
            assert!(
                file_name.starts_with("vendor/") && file_name.ends_with(".js"),
                "{file_name}"
            );
            assert!(
                entry["runtimeIdentifier"].as_str().is_some(),
                "every entry carries the runtime identifier"
            );
        }
    }

    /// The structuralSignature family's members are keyed by the emitted
    /// vendor file path (not the TS writer's collapsed "fresh" keys).
    fn assert_family_keyed_by_file_path(partitions_path: &Path, members_expected: usize) {
        let partitions: Value =
            serde_json::from_str(&std::fs::read_to_string(partitions_path).unwrap()).unwrap();
        let family = &partitions["families"][0];
        assert_eq!(family["family"], "structuralSignature");
        let members = family["members"].as_array().unwrap();
        assert_eq!(members.len(), members_expected);
        for member in members {
            let text = member["member"]["text"].as_str().unwrap();
            assert!(
                text.starts_with("vendor/"),
                "the member key is the file path, got {text}"
            );
        }
    }

    fn assert_source_count(report: &VendorNamesReport, source: &str, minimum: usize) {
        assert!(
            report.name_sources.get(source).copied().unwrap_or(0) >= minimum,
            "{source} should have fired — got {:?}",
            report.name_sources
        );
    }

    #[test]
    fn gate_rebuilds_manifest_and_family_then_is_stable() {
        let root = temp_root("e2e");
        let ts = root.join("ts-dump");
        write_ts_dump(&ts);

        // First run: no prior, no namer — every name lands on the fallback
        // tier and the manifest is written in bundle order.
        let a = root.join("a");
        let report_a = run(&ts, &a, None, None);
        assert_eq!(report_a.factories, 4, "the fixture's four factory calls");
        assert_source_count(&report_a, "fallback", 1);
        assert_manifest_written(&a.join("manifest.json"), 4);
        assert_family_keyed_by_file_path(&a.join("partitions.json"), report_a.family_members);

        // Second run: the first run's manifest as the PRIOR carries the
        // names over (the hash-named records take the prior names).
        let b = root.join("b");
        let report_b = run(&ts, &b, Some(&a.join("manifest.json")), None);
        assert_source_count(&report_b, "carry-over", 1);

        // Third run: the SAME inputs, diffed against the second run's
        // manifest — the rebuild is deterministic.
        let c = root.join("c");
        let report_c = run(
            &ts,
            &c,
            Some(&a.join("manifest.json")),
            Some(&b.join("manifest.json")),
        );
        assert!(
            report_c.manifest_divergences.is_empty(),
            "the rebuild is deterministic: {:?}",
            report_c.manifest_divergences
        );

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn gate_refuses_a_missing_dump_loudly() {
        let root = temp_root("missing");
        let err = dump_vendor_names(
            &root.join("nope"),
            None,
            &root.join("out"),
            VendorNamesGate {
                namer: None,
                collapsed_member_keys: false,
                expect_manifest: None,
            },
        )
        .unwrap_err();
        assert!(err.contains("meta.json"), "{err}");
        std::fs::remove_dir_all(&root).ok();
    }
}
