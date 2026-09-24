//! Vendor-naming unit tests — synthetic mirrors of the TS originals'
//! expectations (src/unpack/adapters/bun.test.ts, src/unpack/
//! vendor-namer.test.ts, src/unpack/manifest-order.test.ts) plus the
//! cache-replay key vector generated from the TS itself (see the
//! `cache_key` test's comment for how the vector was produced).

use std::collections::{HashMap, HashSet};

use oxc_span::Span;

use crate::modules::vendor_names::{
    CacheKeyParams, CacheReplayNamer, ManifestEntry, PriorManifestEntry, VendorNameRequest,
    VendorNamer, accept_vendor_name, annotate_hash_ordinals, build_prompt, build_vendor_evidence,
    choose_file_names, is_vendor_worthy_binding, load_prior_vendor_names,
    name_fallback_factories_with_llm_sized, order_by_prior_manifest, sanitize_fs_name,
    sanitize_fs_path, strip_js_extension, unique_case_insensitive_name, vendor_stem_for,
};
use crate::modules::{FactoryRecord, NameSource};

/// A synthetic classified factory (the TS vendor-namer.test.ts `record`
/// helper's shape).
fn rec(structural_hash: &str, name: &str, source: NameSource) -> FactoryRecord {
    FactoryRecord {
        factory_var: format!("q_{}", structural_hash),
        span: Span::new(0, 10),
        body_span: Span::new(0, 10),
        line_range: (1, 1),
        content_hash: "00000000".to_string(),
        structural_hash: structural_hash.to_string(),
        banner_text: None,
        banner_package: None,
        banner_version: None,
        name: Some(name.to_string()),
        name_source: Some(source),
    }
}

fn fallback_rec(structural_hash: &str) -> FactoryRecord {
    rec(
        structural_hash,
        &format!("lib_{}", &structural_hash[..8]),
        NameSource::Fallback,
    )
}

// ---------------------------------------------------------------------------
// The filename floor (shared/cjs-factory.ts)
// ---------------------------------------------------------------------------

/// Minified residue is never vendor-worthy (the TS floor's own doc).
#[test]
fn vendor_worthy_binding_floor() {
    assert!(is_vendor_worthy_binding("axios"));
    assert!(is_vendor_worthy_binding("$a1"));
    assert!(!is_vendor_worthy_binding("H"));
    assert!(!is_vendor_worthy_binding("qA"));
    assert!(!is_vendor_worthy_binding(""));
    assert!(!is_vendor_worthy_binding("ab"));
}

/// A trailing .js is stripped case-insensitively; a non-.js tail stays.
#[test]
fn strip_js_extension_is_case_insensitive() {
    assert_eq!(strip_js_extension("highlight.js"), "highlight");
    assert_eq!(strip_js_extension("highlight.JS"), "highlight");
    assert_eq!(strip_js_extension("axios"), "axios");
    assert_eq!(strip_js_extension("x.min.js"), "x.min");
}

/// Minified residue floors to lib_<sha256(body)[:8]> (vendorStemFor).
#[test]
fn vendor_stem_floors_minified_residue() {
    let stem = vendor_stem_for("H", "body");
    assert_eq!(stem.len(), "lib_".len() + 8);
    assert!(stem.starts_with("lib_"));
    assert_eq!(vendor_stem_for("axios", "body"), "axios");
}

// ---------------------------------------------------------------------------
// File-name sanitization + uniquify
// ---------------------------------------------------------------------------

/// `/` becomes `__` inside one segment; the path form keeps it a separator.
#[test]
fn sanitize_fs_name_and_path() {
    assert_eq!(sanitize_fs_name("@scope/pkg@1.0.0"), "@scope__pkg@1.0.0");
    assert_eq!(sanitize_fs_name("a b/c"), "a_b__c");
    assert_eq!(sanitize_fs_path("@scope/pkg"), "@scope/pkg");
    assert_eq!(sanitize_fs_path("a//b"), "a/b");
}

/// Case-folding uniquify appends -2, -3 (shared/unique-name.ts).
#[test]
fn unique_case_insensitive_name_suffixes() {
    let mut used = HashSet::new();
    assert_eq!(unique_case_insensitive_name("Ab", &mut used), "Ab");
    assert_eq!(unique_case_insensitive_name("aB", &mut used), "aB-2");
    assert_eq!(unique_case_insensitive_name("AB", &mut used), "AB-3");
}

// ---------------------------------------------------------------------------
// choose_file_names — the TS bun.test.ts naming cases
// ---------------------------------------------------------------------------

/// Fallback names stay flat (vendor/lib_<hash>.js, -2 on a hash-twin).
#[test]
fn fallback_names_stay_flat() {
    let factories = vec![
        fallback_rec("aaaabbbb00000000"),
        fallback_rec("aaaabbbb00000000"),
    ];
    let lookups = choose_file_names(&factories);
    assert_eq!(lookups[0].file_name, "lib_aaaabbbb");
    assert_eq!(lookups[1].file_name, "lib_aaaabbbb-2");
}

/// Banner names become the file name verbatim (sanitized), single module
/// stays flat; a name ending .js does not double it.
#[test]
fn banner_names_become_file_names() {
    let factories = vec![
        rec("aaaabbbb00000000", "axios@1.2.3", NameSource::Banner),
        rec("ccccdddd00000000", "highlight.js", NameSource::Banner),
    ];
    let lookups = choose_file_names(&factories);
    assert_eq!(lookups[0].file_name, "axios@1.2.3");
    // stripJsExtension runs before the .js is appended by the caller.
    assert_eq!(lookups[1].file_name, "highlight");
}

/// A package that identified >=2 modules groups into vendor/<package>/,
/// each module named by its stable structural stem.
#[test]
fn grouped_package_uses_a_folder_with_stable_stems() {
    let factories = vec![
        rec("aaaabbbb00000000", "axios@1.0.0", NameSource::Banner),
        rec("ccccdddd00000000", "axios@1.0.0", NameSource::Banner),
        rec("eeeeffff00000000", "axios@1.0.0", NameSource::Banner),
    ];
    let lookups = choose_file_names(&factories);
    let names: Vec<&str> = lookups.iter().map(|l| l.file_name.as_str()).collect();
    assert_eq!(
        names,
        vec![
            "axios@1.0.0/lib_aaaabbbb",
            "axios@1.0.0/lib_ccccdddd",
            "axios@1.0.0/lib_eeeeffff"
        ]
    );
}

/// A carried-over HASH name stays flat, like a freshly minted one — the
/// grouping gate is on the name, not the name source (the TS test that
/// caught the one-hop folder flip).
#[test]
fn carried_hash_name_stays_flat() {
    let factories = vec![
        rec("aaaabbbb00000000", "lib_aaaabbbb", NameSource::CarryOver),
        rec("aaaabbbb00000000", "lib_aaaabbbb", NameSource::CarryOver),
    ];
    let lookups = choose_file_names(&factories);
    assert_eq!(lookups[0].file_name, "lib_aaaabbbb");
    assert_eq!(lookups[1].file_name, "lib_aaaabbbb-2");
}

/// Case-colliding names disambiguate on disk (case-insensitive FS safe).
#[test]
fn case_collision_suffixes_the_second() {
    let factories = vec![
        rec("aaaabbbb00000000", "Ab@1.0.0", NameSource::Banner),
        rec("ccccdddd00000000", "aB@1.0.0", NameSource::Banner),
    ];
    let lookups = choose_file_names(&factories);
    assert_eq!(lookups[0].file_name, "Ab@1.0.0");
    assert_eq!(lookups[1].file_name, "aB@1.0.0-2");
}

/// A scoped package name becomes a nested folder when grouped.
#[test]
fn scoped_name_becomes_a_nested_folder() {
    let factories = vec![
        rec("aaaabbbb00000000", "@scope/pkg", NameSource::Url),
        rec("ccccdddd00000000", "@scope/pkg", NameSource::Url),
    ];
    let lookups = choose_file_names(&factories);
    assert_eq!(lookups[0].file_name, "@scope/pkg/lib_aaaabbbb");
    assert_eq!(lookups[1].file_name, "@scope/pkg/lib_ccccdddd");
}

// ---------------------------------------------------------------------------
// Manifest assembly — annotate_hash_ordinals + order_by_prior_manifest
// ---------------------------------------------------------------------------

fn entry(name: &str, hash: &str, ordinal: Option<usize>) -> ManifestEntry {
    ManifestEntry {
        file_name: format!("vendor/{name}.js"),
        name: name.to_string(),
        name_source: "carry-over",
        structural_hash: hash.to_string(),
        runtime_identifier: None,
        hash_ordinal: ordinal,
        banner_package: None,
        banner_version: None,
    }
}

/// Singletons are left alone; group members are stamped 0..n in the order
/// given (bundle order at the call site).
#[test]
fn hash_ordinals_stamp_groups_only() {
    let entries = vec![
        entry("retry", "dup", None),
        entry("unrelated", "solo", None),
        entry("lodash", "dup", None),
    ];
    let stamped = annotate_hash_ordinals(entries);
    assert_eq!(stamped[0].hash_ordinal, Some(0));
    assert_eq!(stamped[1].hash_ordinal, None);
    assert_eq!(stamped[2].hash_ordinal, Some(1));
}

/// Pass 1 (structuralHash): an unchanged library returns to its prior slot.
#[test]
fn order_follows_the_prior_by_hash() {
    let prior = vec![
        PriorManifestEntry {
            name: "lodash".to_string(),
            structural_hash: "h2".to_string(),
        },
        PriorManifestEntry {
            name: "retry".to_string(),
            structural_hash: "h1".to_string(),
        },
    ];
    let fresh = vec![entry("retry", "h1", None), entry("lodash", "h2", None)];
    let ordered = order_by_prior_manifest(fresh, Some(&prior));
    assert_eq!(ordered[0].name, "lodash");
    assert_eq!(ordered[1].name, "retry");
}

/// Pass 2 (name): the hash rotated but the carried-over name held.
#[test]
fn order_follows_the_prior_by_name() {
    let prior = vec![PriorManifestEntry {
        name: "js-yaml".to_string(),
        structural_hash: "old".to_string(),
    }];
    let fresh = vec![entry("js-yaml", "new", None)];
    let ordered = order_by_prior_manifest(fresh, Some(&prior));
    assert_eq!(ordered[0].name, "js-yaml");
}

/// Pass 3 (positional): the leftovers pair in order; a genuinely new entry
/// trails the last anchored entry that preceded it.
#[test]
fn leftovers_pair_positionally() {
    let prior = vec![
        PriorManifestEntry {
            name: "a".to_string(),
            structural_hash: "ha".to_string(),
        },
        PriorManifestEntry {
            name: "b".to_string(),
            structural_hash: "hb".to_string(),
        },
    ];
    let fresh = vec![
        entry("a", "ha", None),
        entry("changed-b", "hc", None),
        entry("b", "hb", None),
    ];
    let ordered = order_by_prior_manifest(fresh, Some(&prior));
    // "changed-b" has no hash/name match: it takes the leftover slot AFTER
    // "a"'s — staying beside the entries it shipped with.
    assert_eq!(
        ordered.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(),
        vec!["a", "changed-b", "b"]
    );
}

/// No prior (or an empty one) keeps bundle order.
#[test]
fn no_prior_keeps_bundle_order() {
    let fresh = vec![entry("a", "h", None), entry("b", "g", None)];
    let ordered = order_by_prior_manifest(fresh.clone(), None);
    assert_eq!(ordered[0].name, "a");
    assert_eq!(ordered[1].name, "b");
    let empty: Vec<PriorManifestEntry> = Vec::new();
    let ordered = order_by_prior_manifest(fresh, Some(&empty));
    assert_eq!(ordered[0].name, "a");
}

// ---------------------------------------------------------------------------
// load_prior_vendor_names — the hashOrdinal re-sort
// ---------------------------------------------------------------------------

/// The manifest is written in the PRIOR release's order, so array position
/// no longer encodes the tie-break; hashOrdinal recovers it.
#[test]
fn prior_names_recover_bundle_order_from_hash_ordinal() {
    let manifest = r#"{"adapter":"bun","factories":[
        {"fileName":"vendor/lodash.js","name":"lodash","nameSource":"carry-over","structuralHash":"dup","hashOrdinal":1},
        {"fileName":"vendor/unrelated.js","name":"unrelated","nameSource":"carry-over","structuralHash":"solo"},
        {"fileName":"vendor/retry.js","name":"retry","nameSource":"carry-over","structuralHash":"dup","hashOrdinal":0}
    ]}"#;
    let names = load_prior_vendor_names(manifest).expect("names");
    assert_eq!(names["dup"], vec!["retry", "lodash"]);
    assert_eq!(names["solo"], vec!["unrelated"]);
}

/// A legacy manifest (pre-exp047) has no hashOrdinal and array order IS
/// bundle order.
#[test]
fn legacy_prior_manifest_keeps_array_order() {
    let manifest = r#"{"adapter":"bun","factories":[
        {"fileName":"vendor/retry.js","name":"retry","nameSource":"carry-over","structuralHash":"dup"},
        {"fileName":"vendor/lodash.js","name":"lodash","nameSource":"carry-over","structuralHash":"dup"}
    ]}"#;
    let names = load_prior_vendor_names(manifest).expect("names");
    assert_eq!(names["dup"], vec!["retry", "lodash"]);
}

/// A partially-annotated group sorts the un-annotated member last, and it
/// must not vanish.
#[test]
fn partially_annotated_group_keeps_every_member() {
    let manifest = r#"{"adapter":"bun","factories":[
        {"fileName":"vendor/b.js","name":"b","nameSource":"carry-over","structuralHash":"dup","hashOrdinal":1},
        {"fileName":"vendor/fresh.js","name":"fresh","nameSource":"fallback","structuralHash":"dup"},
        {"fileName":"vendor/a.js","name":"a","nameSource":"carry-over","structuralHash":"dup","hashOrdinal":0}
    ]}"#;
    let names = load_prior_vendor_names(manifest).expect("names");
    assert_eq!(names["dup"], vec!["a", "b", "fresh"]);
}

// ---------------------------------------------------------------------------
// build_vendor_evidence — byte-exact prompt evidence
// ---------------------------------------------------------------------------

/// Collects export names, urls, and distinctive strings; drops too-short
/// strings (the TS vendor-namer.test.ts case).
#[test]
fn evidence_collects_exports_urls_strings() {
    let body = [
        "exports.parse = function () {};",
        "exports.stringify = q;",
        "var msg = \"YAMLException: bad indent\";",
        "var site = \"https://github.com/nodeca/js-yaml\";",
        "var x = \"ab\";",
    ]
    .join("\n");
    let evidence = build_vendor_evidence(&body, 700);
    assert!(evidence.contains("exports: parse, stringify"), "{evidence}");
    assert!(
        evidence.contains("urls: https://github.com/nodeca/js-yaml"),
        "{evidence}"
    );
    assert!(evidence.contains("YAMLException"), "{evidence}");
    assert!(
        !evidence.contains("\"ab\""),
        "too-short strings are noise: {evidence}"
    );
    // The block ends with the UTF-16 size line.
    assert!(evidence.ends_with(&format!("size: {} bytes", body.encode_utf16().count())));
}

/// The cap truncates to `cap` UTF-16 units (the TS `.slice(0, capChars)`).
#[test]
fn evidence_cap_truncates() {
    let body = "exports.parse = function () {};exports.stringify = q;".repeat(50);
    let evidence = build_vendor_evidence(&body, 40);
    assert_eq!(evidence.encode_utf16().count(), 40);
    assert!(
        evidence.starts_with("exports: parse, stringify"),
        "{evidence}"
    );
}

/// The exports regex needs the `=`; an `exports.x;` is not evidence.
#[test]
fn evidence_exports_require_the_assignment() {
    let body = "exports.readonly;\nexports.write=1;";
    let evidence = build_vendor_evidence(body, 700);
    assert!(evidence.contains("exports: write"), "{evidence}");
    assert!(!evidence.contains("readonly"), "{evidence}");
}

// ---------------------------------------------------------------------------
// accept_vendor_name
// ---------------------------------------------------------------------------

#[test]
fn accepts_package_shaped_names_normalized() {
    assert_eq!(accept_vendor_name("js-yaml"), Some("js-yaml".to_string()));
    assert_eq!(
        accept_vendor_name("@aws-sdk/client-s3"),
        Some("@aws-sdk/client-s3".to_string())
    );
    assert_eq!(accept_vendor_name("Zod"), Some("zod".to_string()));
}

#[test]
fn rejects_generic_minified_or_malformed_proposals() {
    for bad in [
        "lib", "library", "utils", "unknown", "module", "index", "vendor", "H", "a b c", "",
    ] {
        assert_eq!(accept_vendor_name(bad), None, "{bad} must be rejected");
    }
}

/// The shape regex bounds: 3..=40 chars, no trailing junk, scope needs a
/// package part.
#[test]
fn rejects_shape_violations() {
    for bad in [
        "ab",
        "@scope",
        "@scope/x",
        "-abc",
        "ok@name",
        "ok-name-trailing!",
    ] {
        assert_eq!(accept_vendor_name(bad), None, "{bad} must be rejected");
    }
}

// ---------------------------------------------------------------------------
// build_prompt — the batch prompt, byte-exact
// ---------------------------------------------------------------------------

/// Locked to the TS `buildPrompt` output (the same string the cache-key
/// vector below was generated with).
#[test]
fn prompt_matches_the_ts_bytes() {
    let requests = vec![
        VendorNameRequest {
            key: "lib_aaaabbbb".to_string(),
            evidence: "exports: parse\nsize: 10 bytes".to_string(),
        },
        VendorNameRequest {
            key: "lib_ccccdddd".to_string(),
            evidence: "strings: \"YAMLException\"\nsize: 24 bytes".to_string(),
        },
    ];
    assert_eq!(
        build_prompt(&requests),
        "Identify 2 vendored modules extracted from a JavaScript bundle.\n\n\
         ### lib_aaaabbbb\nexports: parse\nsize: 10 bytes\n\n\
         ### lib_ccccdddd\nstrings: \"YAMLException\"\nsize: 24 bytes\n\n\
         Reply with JSON {\"lib_aaaabbbb\": \"<npm package or kebab-case name>\", \
         \"lib_ccccdddd\": \"<npm package or kebab-case name>\"}."
    );
}

// ---------------------------------------------------------------------------
// name_fallback_factories_with_llm — the pass over the fallback records
// ---------------------------------------------------------------------------

/// A counting namer: records each batch's keys and answers from a table.
struct RecordingNamer {
    batches: Vec<Vec<String>>,
    answer: String,
}

impl VendorNamer for RecordingNamer {
    fn name_batch(&mut self, requests: Vec<VendorNameRequest>) -> Vec<Option<String>> {
        self.batches
            .push(requests.iter().map(|r| r.key.clone()).collect());
        Some(self.answer.clone())
            .into_iter()
            .cycle()
            .take(requests.len())
            .map(Some)
            .collect()
    }
}

/// Only fallback-named records go through the namer; trusted sources win
/// (the cascade priority is upstream, but the pass must not touch them).
#[test]
fn llm_pass_only_touches_fallback_records() {
    let mut factories = vec![
        rec("aaaabbbb00000000", "axios@1.0.0", NameSource::Banner),
        fallback_rec("ccccdddd00000000"),
        fallback_rec("eeeeffff00000000"),
    ];
    let mut namer = RecordingNamer {
        batches: Vec::new(),
        answer: "js-yaml".to_string(),
    };
    let renamed =
        name_fallback_factories_with_llm_sized(&mut factories, "0123456789", &mut namer, 24, 40);
    assert_eq!(renamed, 2);
    assert_eq!(
        namer.batches.len(),
        1,
        "one batch for the fallback factories"
    );
    assert_eq!(namer.batches[0], vec!["lib_ccccdddd", "lib_eeeeffff"]);
    assert_eq!(factories[0].name.as_deref(), Some("axios@1.0.0"));
    assert_eq!(factories[0].name_source, Some(NameSource::Banner));
    assert_eq!(factories[1].name.as_deref(), Some("js-yaml"));
    assert_eq!(factories[1].name_source, Some(NameSource::Llm));
}

/// Chunking: 30 fallbacks at the default size 24 arrive as two batches.
#[test]
fn llm_pass_chunks_at_24() {
    let mut factories: Vec<FactoryRecord> = (0..30)
        .map(|i| fallback_rec(&format!("{i:016x}")))
        .collect();
    let mut namer = RecordingNamer {
        batches: Vec::new(),
        answer: "js-yaml".to_string(),
    };
    let renamed =
        name_fallback_factories_with_llm_sized(&mut factories, "0123456789", &mut namer, 24, 40);
    assert_eq!(renamed, 30);
    assert_eq!(namer.batches.len(), 2);
    assert_eq!(namer.batches[0].len(), 24);
    assert_eq!(namer.batches[1].len(), 6);
}

/// A name the model applies to more than the cap is a hallucinated default
/// — ALL of its applications revert to the honest lib_<hash> (the TS
/// vendor-namer.test.ts over-application case).
#[test]
fn over_applied_name_reverts_fully() {
    let mut factories: Vec<FactoryRecord> = (0..40)
        .map(|i| fallback_rec(&format!("{i:016x}")))
        .collect();
    let mut namer = RecordingNamer {
        batches: Vec::new(),
        answer: "is-plain-object".to_string(),
    };
    let renamed =
        name_fallback_factories_with_llm_sized(&mut factories, "0123456789", &mut namer, 100, 10);
    assert_eq!(renamed, 0, "an over-applied name is fully reverted");
    assert!(
        factories
            .iter()
            .all(|r| r.name_source == Some(NameSource::Fallback))
    );
}

/// A name applied within the cap keeps every module named (a real package
/// with many modules).
#[test]
fn within_cap_name_keeps() {
    let mut factories: Vec<FactoryRecord> =
        (0..8).map(|i| fallback_rec(&format!("{i:016x}"))).collect();
    let mut namer = RecordingNamer {
        batches: Vec::new(),
        answer: "protobufjs".to_string(),
    };
    let renamed =
        name_fallback_factories_with_llm_sized(&mut factories, "0123456789", &mut namer, 100, 10);
    assert_eq!(renamed, 8);
    assert!(
        factories
            .iter()
            .all(|r| r.name.as_deref() == Some("protobufjs")
                && r.name_source == Some(NameSource::Llm))
    );
}

/// A declined (None) proposal keeps the lib_<hash> fallback. The PASS has
/// no echo check — the real namer nulls echoes upstream (classifyProposal),
/// so a package-shaped echo that leaks through the trait is applied, the
/// TS's own behavior; this locks THAT, not a guess.
#[test]
fn declined_and_echoed_proposals_keep_the_fallback() {
    struct EchoNamer;
    impl VendorNamer for EchoNamer {
        fn name_batch(&mut self, requests: Vec<VendorNameRequest>) -> Vec<Option<String>> {
            // Echo the key for the first, decline the second.
            vec![Some(requests[0].key.clone()), None]
        }
    }
    let mut factories = vec![
        fallback_rec("aaaabbbb00000000"),
        fallback_rec("ccccdddd00000000"),
    ];
    let renamed = name_fallback_factories_with_llm_sized(
        &mut factories,
        "0123456789",
        &mut EchoNamer,
        24,
        40,
    );
    assert_eq!(
        renamed, 1,
        "the echo is package-shaped, so the pass applies it"
    );
    assert_eq!(factories[0].name.as_deref(), Some("lib_aaaabbbb"));
    assert_eq!(factories[0].name_source, Some(NameSource::Llm));
    assert_eq!(factories[1].name.as_deref(), Some("lib_ccccdddd"));
    assert_eq!(factories[1].name_source, Some(NameSource::Fallback));
}

// ---------------------------------------------------------------------------
// CacheReplayNamer — the LLM boundary's parity wiring
// ---------------------------------------------------------------------------

/// The cache key vector, generated by running the TS `cacheKeyOf` over this
/// exact request (npx tsx, src/llm/cached-provider.ts, params {model:
/// "openai/gpt-oss-20b", temperature: 0}, the REAL SYSTEM_PROMPT from
/// src/unpack/vendor-namer.ts — the same string this port bakes in):
///
///   {"key":"113acd85bbf7aac27c2ca29f774f82aaad128f52f32ae5230b8df0f5ef827dfb",
///    "prompt":"Identify 2 vendored modules ..."}
///
/// The prompt bytes are locked separately by `prompt_matches_the_ts_bytes`.
#[test]
fn cache_key_matches_the_ts_vector() {
    let namer = CacheReplayNamer::new(
        std::env::temp_dir(),
        CacheKeyParams {
            model: "openai/gpt-oss-20b".to_string(),
            temperature: 0,
            max_tokens: None,
            reasoning_effort: None,
        },
    );
    let requests = vec![
        VendorNameRequest {
            key: "lib_aaaabbbb".to_string(),
            evidence: "exports: parse\nsize: 10 bytes".to_string(),
        },
        VendorNameRequest {
            key: "lib_ccccdddd".to_string(),
            evidence: "strings: \"YAMLException\"\nsize: 24 bytes".to_string(),
        },
    ];
    assert_eq!(
        namer.cache_key_of(&requests),
        "113acd85bbf7aac27c2ca29f774f82aaad128f52f32ae5230b8df0f5ef827dfb"
    );
}

/// The oracle's own recorded vector: the dump's cache-keys.jsonl first row
/// — the 17-factory vendor batch of the 2.1.215-2.1.216 oracle run — replays
/// to its recorded key `dd639026…f3f3` with params {model "openai/
/// gpt-oss-20b", temperature 0, reasoningEffort "low"}. The evidence list is
/// the dump's own byte-exact per-factory evidence; this is the full key
/// material (params + request) verified end to end.
#[test]
fn cache_key_matches_the_oracle_recorded_vector() {
    let namer = CacheReplayNamer::new(
        std::env::temp_dir(),
        CacheKeyParams {
            model: "openai/gpt-oss-20b".to_string(),
            temperature: 0,
            max_tokens: None,
            reasoning_effort: Some("low".to_string()),
        },
    );
    let batch: &[(&str, &str)] = &[
        (
            "lib_c0dcf0b5",
            "strings: \"__esModule\", \"$dynamicAnchor\", \"string\"\nsize: 736 bytes",
        ),
        (
            "lib_4bdd543c",
            "strings: \"__esModule\", \"$dynamicRef\", \"string\", \")throw Error(`\", \"valid\"\nsize: 732 bytes",
        ),
        (
            "lib_a85f54f3",
            "strings: \"__esModule\", \"$recursiveAnchor\", \"boolean\", \");else(0,_r_.checkStrictMode)(e.it,\"\nsize: 285 bytes",
        ),
        (
            "lib_83642267",
            "strings: \"__esModule\", \"$recursiveRef\", \"string\"\nsize: 193 bytes",
        ),
        ("lib_8110ff70", "strings: \"__esModule\"\nsize: 186 bytes"),
        (
            "lib_b02a2cf6",
            "strings: \"__esModule\", \"dependentRequired\", \"object\"\nsize: 228 bytes",
        ),
        (
            "lib_7ca10018",
            "strings: \"__esModule\", \"dependentSchemas\", \"object\"\nsize: 209 bytes",
        ),
        (
            "lib_daa6b6e5",
            "strings: \"__esModule\", \"maxContains\", \"minContains\", \"array\", \"number\", \"${e}\", \"contains\"\nsize: 306 bytes",
        ),
        ("lib_18fb1058", "strings: \"__esModule\"\nsize: 164 bytes"),
        (
            "lib_1b161656",
            "strings: \"__esModule\", \"must NOT have unevaluated properties\", \"unevaluatedProperties\", \"object\", \"boolean\", \"ajv implementation error\", \",n,(d)=>t.if(c(a,d),()=>l(d))));else if(a!==!0)t.forIn(\", \"valid\"\nsize: 1155 bytes",
        ),
        (
            "lib_98bc5538",
            "strings: \"__esModule\", \"unevaluatedItems\", \"array\", \"boolean\", \"object\", \"valid\", \",c,s,(u)=>{if(e.subschema({keyword:\"\nsize: 824 bytes",
        ),
        ("lib_f00ebf1d", "strings: \"__esModule\"\nsize: 142 bytes"),
        ("lib_615a4813", "strings: \"__esModule\"\nsize: 315 bytes"),
        (
            "lib_f8c7b87b",
            "strings: \"__esModule\", \"/properties\"\nsize: 368 bytes",
        ),
        (
            "lib_87a9bbc1",
            "strings: \"None configured\", \"sensitive\", \"inline it\", \"hardcode it\", \"t already requires the class\", \"s or organization\", \"s own Edit/Write content before a commit: a hobbyist\", \"s username (a username in a workload\", \"s own words name the action and the rule\", \" answered yes meets it. A rule\"\nsize: 75509 bytes",
        ),
        (
            "lib_82d5b93a",
            "strings: \"#256abf,#199e70,...\", \"#1a1a19\", \"#86b6ef,#5598e7,#256abf,#104281\", \"#2a78d6,#eb6834,...\", \"light\", \"module\", \"validate_palette.js\", \"#fcfcfb\", \").split(\", \"adjacent\"\nsize: 19660 bytes",
        ),
        (
            "lib_3be10b08",
            "strings: \"#256abf,#199e70,...\", \"#1a1a19\", \"light\", \"dark\", \"#fcfcfb\", \"protan\", \"deutan\", \"tritan\", \").split(\", \"#?[0-9a-fA-F]{6}\"\nsize: 17283 bytes",
        ),
    ];
    let requests: Vec<VendorNameRequest> = batch
        .iter()
        .map(|(key, evidence)| VendorNameRequest {
            key: (*key).to_string(),
            evidence: (*evidence).to_string(),
        })
        .collect();
    assert_eq!(
        namer.cache_key_of(&requests),
        "dd639026f1a8847fc1617ad1cccdfa95b50109b13067dc3b44f7cd674f7e3f3f"
    );
}

/// A cache hit returns the entry's renames; a miss is counted and answers
/// all-null (an all-null oracle batch is never cached — see the type doc).
#[test]
fn replay_reads_the_sharded_entry_and_counts_misses() {
    let dir = std::env::temp_dir().join(format!("humanify-vendor-test-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    let mut namer = CacheReplayNamer::new(
        dir.clone(),
        CacheKeyParams {
            model: "openai/gpt-oss-20b".to_string(),
            temperature: 0,
            max_tokens: None,
            reasoning_effort: None,
        },
    );
    let requests = vec![
        VendorNameRequest {
            key: "lib_aaaabbbb".to_string(),
            evidence: "exports: parse\nsize: 10 bytes".to_string(),
        },
        VendorNameRequest {
            key: "lib_ccccdddd".to_string(),
            evidence: "strings: \"YAMLException\"\nsize: 24 bytes".to_string(),
        },
    ];
    // The shard the replay reads, keyed exactly the way the namer computes it.
    let key = namer.cache_key_of(&requests);
    std::fs::create_dir_all(dir.join(&key[..2])).unwrap();
    std::fs::write(
        dir.join(&key[..2]).join(format!("{}.json", &key[2..])),
        r#"{"v":1,"renames":{"lib_aaaabbbb":"js-yaml","lib_ccccdddd":null}}"#,
    )
    .unwrap();
    let answers = namer.name_batch(requests.clone());
    assert_eq!(answers[0].as_deref(), Some("js-yaml"));
    assert_eq!(answers[1], None, "an explicit null rename is a decline");
    assert_eq!(namer.misses, 0);

    // A different prompt (different evidence) misses.
    let mut other = requests;
    other[0].evidence = "different".to_string();
    let answers = namer.name_batch(other);
    assert!(answers.iter().all(|a| a.is_none()));
    assert_eq!(namer.misses, 1);
    std::fs::remove_dir_all(&dir).ok();
}

// ---------------------------------------------------------------------------
// The deterministic cascade end-to-end through the prior-name plumbing
// ---------------------------------------------------------------------------

/// The carry-over plumbing: prior names keyed by structuralHash arrive in
/// group order and land on the records positionally (the TS bun.test.ts
/// hash-colliding-shims case).
#[test]
fn carry_over_honours_per_member_names() {
    // Two structurally identical shims + two distinct modules; the prior
    // names the shims differently — each must keep its own.
    let source = r#"var x=(I,A)=>()=>(A||I((A = {exports:{}}).exports, A), A.exports);
var depOne=x((exports,module)=>{ module.exports=function one(a){return a+1}; });
var depTwo=x((exports,module)=>{ module.exports=function two(a,b,c){return a*b*c}; });
var shimOne=x((exports,module)=>{ module.exports=depOne(); });
var shimTwo=x((exports,module)=>{ module.exports=depTwo(); });
var main=shimOne();"#;
    let allocator = oxc_allocator::Allocator::default();
    let ingest = crate::ingest::Ingest::parse(&allocator, source, "input.js");
    assert!(ingest.errors.is_empty(), "fixture must parse");
    let tables = crate::hash::serialize::SymbolTables::build(ingest.semantic());
    let wrapper = crate::modules::wrapper::find_wrapper_function(ingest.program, ingest.semantic());
    let classify = || {
        crate::modules::classify_bun_modules(
            source,
            ingest.program,
            ingest.semantic(),
            wrapper.as_ref().map(|w| w.body_span),
            &tables,
        )
        .expect("a bun bundle")
    };

    // Discover the shims' SHARED structural hash first — the prior names
    // that group, keyed by it.
    let probe = classify();
    let shared_hash = {
        let mut counts: HashMap<&str, usize> = HashMap::new();
        for f in &probe.factories {
            if !f.structural_hash.is_empty() {
                *counts.entry(f.structural_hash.as_str()).or_insert(0) += 1;
            }
        }
        counts
            .into_iter()
            .find(|(_, n)| *n == 2)
            .map(|(h, _)| h.to_string())
            .expect("the two shims share a structural hash")
    };

    let mut prior: HashMap<String, Vec<String>> = HashMap::new();
    prior.insert(
        shared_hash.clone(),
        vec!["retry".to_string(), "lodash".to_string()],
    );
    let mut classification = classify();
    crate::modules::name_cjs_factories(&mut classification, source, Some(&prior));
    let shared: Vec<&FactoryRecord> = classification
        .factories
        .iter()
        .filter(|f| f.structural_hash == shared_hash)
        .collect();
    assert_eq!(shared.len(), 2, "the two shims share a structural hash");
    assert_eq!(
        shared
            .iter()
            .map(|f| f.name.as_deref().unwrap_or(""))
            .collect::<Vec<_>>(),
        vec!["retry", "lodash"],
        "each colliding shim keeps its own prior name"
    );
}
