//! The artifact dump writer (`--dump-artifacts <dir>`) — TS
//! `src/dump/write.ts` `writeDumpArtifacts`: assemble the 07 §2 catalog
//! from what the run recorded and write it, once, after the split and the
//! other reports. Never a second pipeline pass: every decision row comes
//! from the stage that made it — the naming era's capture
//! ([`crate::naming::driver::era::EraCapture`]: the graph, the matches,
//! the twins, the mechanical boundary, the votes), the naming stage's
//! final trail / name records / dispatches, the split's
//! [`SplitSections`], the unpack's vendor-namer calls.
//!
//! Two sections are re-derived here from a text by the same pure owner the
//! run used, because the run keeps no copy: modules.json's classification
//! sites ([`crate::modules::modules_dump::classify_site`] over the
//! minified and the fresh text, with the blessed TS factory-hash injection
//! applied exactly as the unpack stage applies it) and the tree manifest
//! (a walk of the written tree). Spans are UTF-8 bytes throughout (the TS
//! converts its UTF-16 spans at this boundary; the Rust's are bytes
//! already).
//!
//! Files: meta.json, text/{fresh,prior,minified,shipped,generated,
//! reconciled}.js, functions.json, modules.json (Bun only), twins.json /
//! twin-gates.json (prior only), partitions.json, matches.json,
//! matches-close.json (when the close tier ran), transfers.json,
//! transfers-mechanical.json, votes.json, prompts.jsonl, cache-keys.jsonl,
//! names.json, placement.json, emit.json, tree-manifest.json,
//! regions.json.

use std::collections::HashMap;
use std::path::Path;

use humanify_model::dump::{
    EmitLayoutFile, FunctionsFile, MatchPair, MatchRejection, MatchesCloseFile, NamesFile,
    PartitionFamily, PartitionMember, PartitionsFile, SpanKey, TransfersFile, VotesFile,
};
use humanify_model::js::{JsObject, JsValue, cmp_utf16, stringify};
use humanify_model::llm::{BatchRenameRequest, CacheKeyParams, LlmCall, StrMap, cache_key_of};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::naming::driver::NamingOutcome;
use crate::naming::passes::sweep::SweepDispatch;
use crate::naming::report::diagnostics::AnchorTexts;
use crate::naming::waves::processor::DispatchRecord;
use crate::trail::Anchor;

/// `DUMP_SCHEMA_VERSION`.
pub const DUMP_SCHEMA_VERSION: u64 = humanify_model::dump::DUMP_SCHEMA_VERSION;

/// The split's dump sections (`recordStatementHashFamily`, the emit
/// captures, the `folders` prompt site, the placement trail).
pub struct SplitSections {
    /// The statementHash family: each wrapper statement's span in the
    /// shipped text and its hash (the injected TS bytes when injected).
    pub statement_family: Vec<((u32, u32), String)>,
    /// emit.json: the emitted layout that won.
    pub emit: EmitLayoutFile,
    /// The split namers' calls in dispatch order, with their functionId
    /// (`split-namer` / `tree-reviser`).
    pub prompts: Vec<(&'static str, LlmCall)>,
    /// placement.json (`PlacementTrail::placement_json`).
    pub placement: JsValue,
    /// The split's input (the shipped text).
    pub shipped: String,
}

/// What the writer reads.
pub struct DumpInputs<'a> {
    pub dir: &'a Path,
    /// The output tree (the manifest's root, the vendor manifest).
    pub output_dir: &'a Path,
    /// meta.json's `flags` (the CLI's resolved selection).
    pub flags: JsValue,
    pub commit: String,
    pub generated_at: String,
    pub minified: &'a str,
    pub prior: Option<&'a str>,
    pub fresh: &'a str,
    pub outcome: &'a NamingOutcome,
    pub split: Option<&'a SplitSections>,
    /// The vendor namer's calls (the `vendor` site), dispatch order.
    pub vendor_prompts: &'a [LlmCall],
    pub params: &'a CacheKeyParams,
    /// The blessed TS factory-hash injection (the unpack stage's).
    pub ts_factories: Option<&'a [crate::unpack::gate::TsFactoryHash]>,
    /// The processed file's library comment regions (its mixed-file
    /// detection), MINIFIED-text byte offsets.
    pub comment_regions: &'a [crate::libdetect::CommentRegion],
    /// The post-split reconcile's trail rows, per split file (their
    /// spans index the file's text).
    pub extra_trail: &'a crate::naming::report::diagnostics::ExtraTrail,
}

fn sha256_hex(text: &str) -> String {
    Sha256::digest(text.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn span_key(text: &str, start: u32, end: u32) -> SpanKey {
    SpanKey {
        text: text.to_string(),
        start: i64::from(start),
        end: i64::from(end),
    }
}

struct Writer<'d> {
    dir: &'d Path,
}

impl Writer<'_> {
    fn text(&self, file: &str, content: &str) -> Result<(), String> {
        let path = self.dir.join(file);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("mkdir {}: {e}", parent.display()))?;
        }
        std::fs::write(&path, content).map_err(|e| format!("write {}: {e}", path.display()))
    }

    fn json<T: serde::Serialize>(&self, file: &str, value: &T) -> Result<(), String> {
        self.text(
            file,
            &serde_json::to_string(value).map_err(|e| format!("serialize {file}: {e}"))?,
        )
    }
}

/// `writeDumpArtifacts`.
pub fn write_artifact_dump(inp: &DumpInputs<'_>) -> Result<(), String> {
    let w = Writer { dir: inp.dir };
    std::fs::create_dir_all(inp.dir.join("text"))
        .map_err(|e| format!("mkdir {}: {e}", inp.dir.display()))?;
    let out = inp.outcome;
    let texts = DumpTexts {
        fresh: Some(inp.fresh),
        prior: inp.prior,
        minified: Some(inp.minified),
        shipped: inp.split.map(|s| s.shipped.as_str()),
        generated: out.generated.as_deref(),
        reconciled: out.reconcile.as_ref().and_then(|r| r.code.as_deref()),
    };
    write_meta(&w, inp, &texts)?;
    for (name, text) in texts.labelled() {
        if let Some(t) = text {
            w.text(&format!("text/{name}.js"), t)?;
        }
    }
    let capture = out.capture.as_ref();
    if let Some(c) = capture {
        let functions: FunctionsFile = serde_json::from_value(json!({
            "schemaVersion": DUMP_SCHEMA_VERSION,
            "functions": c.functions,
        }))
        .map_err(|e| format!("functions rows: {e}"))?;
        w.json("functions.json", &functions)?;
    }
    // The graph-time classification site (the fresh text's): modules.json's
    // `graph` and regions.json's banner rows.
    let graph_site = crate::modules::modules_dump::classify_site(inp.fresh)
        .map_err(|e| format!("fresh: {e}"))?;
    write_modules(&w, inp, graph_site.as_ref())?;
    if let Some(m) = capture.and_then(|c| c.matches.as_ref()) {
        write_twin_gates(&w, &m.twin_gates)?;
        write_twins(&w, &m.twins)?;
    }
    write_partitions(&w, inp)?;
    write_matches(&w, inp)?;
    let extra = extra_keys(inp.extra_trail, texts.generated)?;
    let mut transfers = out.trail.transfer_rows();
    transfers.extend(extra.iter().map(|x| x.entry.transfer_row(x.key.clone())));
    transfers.sort_by(|a, b| a.target.cmp(&b.target));
    w.json(
        "transfers.json",
        &TransfersFile {
            schema_version: DUMP_SCHEMA_VERSION,
            transfers,
        },
    )?;
    if let Some(c) = capture {
        w.json(
            "transfers-mechanical.json",
            &TransfersFile {
                schema_version: DUMP_SCHEMA_VERSION,
                transfers: c.mechanical.clone(),
            },
        )?;
    }
    w.json(
        "votes.json",
        &VotesFile {
            schema_version: DUMP_SCHEMA_VERSION,
            votes: crate::rename::votes::dump::vote_rows(
                capture.map_or(&[][..], |c| c.votes.as_slice()),
                &out.trail,
            ),
        },
    )?;
    let (prompts, keys) = dispatch_rows(&run_dispatches(inp), inp.params);
    w.text("prompts.jsonl", &prompts)?;
    w.text("cache-keys.jsonl", &keys)?;
    let names = crate::naming::driver::dump::names_table(
        &out.trail,
        &out.waves.names,
        &out.library_names,
        &AnchorTexts {
            fresh: inp.fresh,
            generated: texts.generated,
            reconciled: texts.reconciled,
            shipped: out.code.as_deref(),
        },
        &extra,
    );
    let names: NamesFile = serde_json::from_value(json!({
        "schemaVersion": DUMP_SCHEMA_VERSION,
        "names": names,
    }))
    .map_err(|e| format!("names rows: {e}"))?;
    w.json("names.json", &names)?;
    let placement = match inp.split {
        Some(s) => stringify(&s.placement),
        None => stringify(&crate::place::trail::PlacementTrail::default().placement_json()),
    };
    w.text("placement.json", &placement)?;
    let empty_emit = EmitLayoutFile {
        schema_version: DUMP_SCHEMA_VERSION,
        files: Vec::new(),
    };
    w.json("emit.json", inp.split.map_or(&empty_emit, |s| &s.emit))?;
    w.text("tree-manifest.json", &tree_manifest(inp.output_dir)?)?;
    write_regions(&w, inp, graph_site.as_ref())
}

/// A classification site: the classification and its wrapper.
type Site = (
    crate::modules::BunModuleClassification,
    Option<crate::modules::wrapper::WrapperFunction>,
);

/// The post-split reconcile's rows keyed as the TS dump keys them (finding
/// #50): the rows carry the reconcile pass's `"generated"` label, so the
/// writer converts their RAW offsets — JS indexes into the SPLIT FILE —
/// through the GENERATED text's UTF-16 → byte table (an offset that
/// leaves the text or lands inside a surrogate pair throws, as the TS
/// table does).
fn extra_keys<'e>(
    extra: &'e crate::naming::report::diagnostics::ExtraTrail,
    generated: Option<&str>,
) -> Result<Vec<crate::naming::driver::dump::ExtraNameRow<'e>>, String> {
    use humanify_model::js::Utf16Offsets;
    let table = generated.map(Utf16ToByte::new);
    let mut out = Vec::new();
    for (text, rows) in extra {
        let offsets = Utf16Offsets::new(text);
        let lines = crate::babel_view::BabelLines::new(text);
        for e in rows {
            let (s, en) = (e.target.decl_span.start, e.target.decl_span.end);
            let (u_s, u_e) = (offsets.at(s), offsets.at(en));
            let (start, end) = match &table {
                Some(t) => (t.to_byte(u_s)?, t.to_byte(u_e)?),
                None => (u_s, u_e),
            };
            let (line, col) = lines.loc(s);
            out.push(crate::naming::driver::dump::ExtraNameRow {
                key: span_key("generated", start, end),
                loc: format!("{line}:{col}"),
                entry: e,
            });
        }
    }
    Ok(out)
}

/// `ByteOffsetTable.toByte` (src/dump/spans.ts): a JS string index of one
/// text as its UTF-8 byte offset.
struct Utf16ToByte {
    /// The text's JS `.length`.
    units: u32,
    /// Per code unit, its byte offset (`INTERIOR` inside a surrogate
    /// pair); None for an ASCII text (the identity).
    table: Option<Vec<u32>>,
}

const INTERIOR: u32 = u32::MAX;

impl Utf16ToByte {
    fn new(text: &str) -> Self {
        if text.is_ascii() {
            return Utf16ToByte {
                units: text.len() as u32,
                table: None,
            };
        }
        let mut table: Vec<u32> = Vec::with_capacity(text.len() + 1);
        for (byte, ch) in text.char_indices() {
            table.push(byte as u32);
            if ch.len_utf16() == 2 {
                table.push(INTERIOR);
            }
        }
        table.push(text.len() as u32);
        Utf16ToByte {
            units: (table.len() - 1) as u32,
            table: Some(table),
        }
    }

    fn to_byte(&self, index: u32) -> Result<u32, String> {
        if index > self.units {
            return Err(format!(
                "span endpoint {index} outside the anchored text (length {})",
                self.units
            ));
        }
        match self.table.as_ref().map(|t| t[index as usize]) {
            None => Ok(index),
            Some(INTERIOR) => Err(format!(
                "span endpoint {index} lands inside a surrogate pair — a dumper bug (07 §1 guard b); refusing to round"
            )),
            Some(b) => Ok(b),
        }
    }
}

/// The anchored texts, by label.
struct DumpTexts<'a> {
    fresh: Option<&'a str>,
    prior: Option<&'a str>,
    minified: Option<&'a str>,
    shipped: Option<&'a str>,
    generated: Option<&'a str>,
    reconciled: Option<&'a str>,
}

impl<'a> DumpTexts<'a> {
    fn labelled(&self) -> [(&'static str, Option<&'a str>); 6] {
        [
            ("fresh", self.fresh),
            ("prior", self.prior),
            ("minified", self.minified),
            ("shipped", self.shipped),
            ("generated", self.generated),
            ("reconciled", self.reconciled),
        ]
    }
}

/// meta.json: schema, when, the CWD's commit, the flags, each anchored
/// text's sha256 (`null` when absent — or empty, the TS's falsy test).
fn write_meta(w: &Writer<'_>, inp: &DumpInputs<'_>, texts: &DumpTexts<'_>) -> Result<(), String> {
    let mut hashes = JsObject::new();
    for (name, text) in texts.labelled() {
        hashes.insert(
            name,
            text.filter(|t| !t.is_empty())
                .map_or(JsValue::Null, |t| JsValue::str(sha256_hex(t))),
        );
    }
    let mut meta = JsObject::new();
    meta.insert("schemaVersion", JsValue::Number(DUMP_SCHEMA_VERSION as f64));
    meta.insert("generatedAt", JsValue::str(inp.generated_at.as_str()));
    meta.insert("commit", JsValue::str(inp.commit.as_str()));
    meta.insert("flags", inp.flags.clone());
    meta.insert("texts", JsValue::Object(hashes));
    w.text("meta.json", &stringify(&JsValue::Object(meta)))
}

/// modules.json (`writeBunModules`): the unpack site (the MINIFIED text's
/// classification, with the TS factory hashes injected as the unpack stage
/// injects them) and the graph site (the fresh text's — None on a real Bun
/// bundle: the beautifier splits the `{exports:{}}` marker). No file when
/// neither site classified.
fn write_modules(w: &Writer<'_>, inp: &DumpInputs<'_>, graph: Option<&Site>) -> Result<(), String> {
    use crate::modules::modules_dump::{classify_site, site_json};
    let mut unpack = classify_site(inp.minified).map_err(|e| format!("minified: {e}"))?;
    if let (Some((c, _)), Some(rows)) = (unpack.as_mut(), inp.ts_factories) {
        crate::unpack::gate::inject_ts_hashes(c, rows)?;
    }
    if unpack.is_none() && graph.is_none() {
        return Ok(());
    }
    let modules: humanify_model::dump::ModulesFile = serde_json::from_value(json!({
        "schemaVersion": DUMP_SCHEMA_VERSION,
        "unpack": site_json(unpack.as_ref().map(|(c, wr)| (c, wr.as_ref())), "minified"),
        "graph": site_json(graph.map(|(c, wr)| (c, wr.as_ref())), "fresh"),
    }))
    .map_err(|e| format!("modules rows: {e}"))?;
    w.json("modules.json", &modules)
}

/// twins.json in the TS writer's key order: `{schemaVersion, inventories:
/// {prior, fresh}, uniqueTier: {uniqueTwins, pairs: [{prior, fresh,
/// hash}]}}`, each inventory `{statements, distinctHashes, uniqueHashes,
/// maxBucket, bucketHistogram}` (its integer keys ascending, as a JS
/// object enumerates them).
fn write_twins(w: &Writer<'_>, twins: &Value) -> Result<(), String> {
    let v = JsValue::parse(&twins.to_string())?;
    let get = |o: &JsValue, k: &str| -> JsValue {
        o.as_object()
            .and_then(|o| o.get(k))
            .cloned()
            .unwrap_or(JsValue::Null)
    };
    let pick = |o: &JsValue, keys: &[&str]| -> JsValue {
        let mut out = JsObject::new();
        for k in keys {
            out.insert(*k, get(o, k));
        }
        JsValue::Object(out)
    };
    let inventories = get(&v, "inventories");
    let inventory_keys = [
        "statements",
        "distinctHashes",
        "uniqueHashes",
        "maxBucket",
        "bucketHistogram",
    ];
    let mut inv = JsObject::new();
    for side in ["prior", "fresh"] {
        inv.insert(side, pick(&get(&inventories, side), &inventory_keys));
    }
    let tier = get(&v, "uniqueTier");
    let pairs = match get(&tier, "pairs") {
        JsValue::Array(items) => items
            .iter()
            .map(|p| {
                let mut o = JsObject::new();
                for k in ["prior", "fresh"] {
                    o.insert(k, pick(&get(p, k), &["text", "start", "end"]));
                }
                o.insert("hash", get(p, "hash"));
                JsValue::Object(o)
            })
            .collect(),
        _ => Vec::new(),
    };
    let mut unique = JsObject::new();
    unique.insert("uniqueTwins", get(&tier, "uniqueTwins"));
    unique.insert("pairs", JsValue::Array(pairs));
    let mut out = JsObject::new();
    out.insert("schemaVersion", JsValue::Number(DUMP_SCHEMA_VERSION as f64));
    out.insert("inventories", JsValue::Object(inv));
    out.insert("uniqueTier", JsValue::Object(unique));
    w.text("twins.json", &stringify(&JsValue::Object(out)))
}

/// twin-gates.json in the TS writer's key order: the stats bag in its
/// literal's order ([`crate::twins::gates::TWIN_GATE_STATS_KEYS`]), the
/// typed rows.
fn write_twin_gates(w: &Writer<'_>, gates: &Value) -> Result<(), String> {
    struct Stats<'a>(&'a Value);
    impl serde::Serialize for Stats<'_> {
        fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            use serde::ser::SerializeMap;
            let keys = crate::twins::gates::TWIN_GATE_STATS_KEYS;
            let mut m = s.serialize_map(Some(keys.len()))?;
            for k in keys {
                m.serialize_entry(k, &self.0[k])?;
            }
            m.end()
        }
    }
    #[derive(serde::Serialize)]
    struct Out<'a> {
        #[serde(rename = "schemaVersion")]
        schema_version: u64,
        stats: Stats<'a>,
        rows: Vec<humanify_model::dump::TwinGateRow>,
    }
    let rows = serde_json::from_value(gates["rows"].clone())
        .map_err(|e| format!("twin gate rows: {e}"))?;
    w.json(
        "twin-gates.json",
        &Out {
            schema_version: DUMP_SCHEMA_VERSION,
            stats: Stats(&gates["stats"]),
            rows,
        },
    )
}

/// partitions.json: the structuralHash family (fresh — the graph capture),
/// the statementHash family (shipped — the split), the structuralSignature
/// family (the written vendor manifest, keyed by FILE PATH with a zero
/// span). Members sorted by span key.
fn write_partitions(w: &Writer<'_>, inp: &DumpInputs<'_>) -> Result<(), String> {
    let mut families = Vec::new();
    let family = |name: &str, mut members: Vec<PartitionMember>| {
        members.sort_by(|a, b| a.member.cmp(&b.member));
        PartitionFamily {
            family: name.to_string(),
            members,
        }
    };
    if let Some(c) = &inp.outcome.capture {
        families.push(family(
            "structuralHash",
            c.structural_family
                .iter()
                .map(|(s, h)| PartitionMember {
                    member: span_key("fresh", s.start, s.end),
                    hash: h.clone(),
                })
                .collect(),
        ));
    }
    if let Some(split) = inp.split {
        families.push(family(
            "statementHash",
            split
                .statement_family
                .iter()
                .map(|((s, e), h)| PartitionMember {
                    member: span_key("shipped", *s, *e),
                    hash: h.clone(),
                })
                .collect(),
        ));
    }
    if let Some(vendor) = vendor_signature_family(inp.output_dir) {
        families.push(family("structuralSignature", vendor));
    }
    w.json(
        "partitions.json",
        &PartitionsFile {
            schema_version: DUMP_SCHEMA_VERSION,
            families,
        },
    )
}

/// `readVendorSignatureFamily`: the written vendor manifest's factories
/// with a structural hash, each keyed by its vendor FILE PATH.
fn vendor_signature_family(output_dir: &Path) -> Option<Vec<PartitionMember>> {
    let text = std::fs::read_to_string(crate::unpack::bun::bun_manifest_path(output_dir)).ok()?;
    let manifest: Value = serde_json::from_str(&text).ok()?;
    let members: Vec<PartitionMember> = manifest["factories"]
        .as_array()?
        .iter()
        .filter_map(|f| {
            let hash = f["structuralHash"].as_str().filter(|h| !h.is_empty())?;
            Some(PartitionMember {
                member: SpanKey {
                    text: f["fileName"].as_str().unwrap_or_default().to_string(),
                    start: 0,
                    end: 0,
                },
                hash: hash.to_string(),
            })
        })
        .collect();
    (!members.is_empty()).then_some(members)
}

/// matches.json (+ matches-close.json when the close tier ran): the
/// capture's cascade rows in the TS order (pairs by prior then fresh,
/// rejections by prior, candidates sorted). Without a prior the file
/// still exists: no rows, null stats.
fn write_matches(w: &Writer<'_>, inp: &DumpInputs<'_>) -> Result<(), String> {
    let sections = inp
        .outcome
        .capture
        .as_ref()
        .and_then(|c| c.matches.as_ref());
    let Some(m) = sections else {
        let mut o = JsObject::new();
        o.insert("schemaVersion", JsValue::Number(DUMP_SCHEMA_VERSION as f64));
        o.insert("resolutionStats", JsValue::Null);
        o.insert("bindingResolutionStats", JsValue::Null);
        o.insert("pairs", JsValue::Array(Vec::new()));
        o.insert("rejections", JsValue::Array(Vec::new()));
        return w.text("matches.json", &stringify(&JsValue::Object(o)));
    };
    let mut pairs: Vec<MatchPair> = serde_json::from_value(m.matches["pairs"].clone())
        .map_err(|e| format!("match pairs: {e}"))?;
    pairs.sort_by(|a, b| a.prior.cmp(&b.prior).then_with(|| a.fresh.cmp(&b.fresh)));
    let mut rejections: Vec<MatchRejection> =
        serde_json::from_value(m.matches["rejections"].clone())
            .map_err(|e| format!("match rejections: {e}"))?;
    for r in &mut rejections {
        if let Some(c) = r.candidates.as_mut() {
            c.sort();
        }
    }
    rejections.sort_by(|a, b| a.prior.cmp(&b.prior));
    /// matches.json in the TS writer's key order.
    #[derive(serde::Serialize)]
    struct MatchesOut<'a> {
        #[serde(rename = "schemaVersion")]
        schema_version: u64,
        #[serde(rename = "resolutionStats")]
        resolution_stats: &'a crate::matching::cascade::ResolutionStats,
        #[serde(rename = "bindingResolutionStats")]
        binding_resolution_stats: Option<&'a crate::matching::cascade::ResolutionStats>,
        pairs: Vec<MatchPair>,
        rejections: Vec<MatchRejection>,
    }
    w.json(
        "matches.json",
        &MatchesOut {
            schema_version: DUMP_SCHEMA_VERSION,
            resolution_stats: &m.resolution_stats,
            binding_resolution_stats: m.binding_resolution_stats.as_ref(),
            pairs,
            rejections,
        },
    )?;
    if let Some(close) = &m.matches_close {
        let mut close: MatchesCloseFile =
            serde_json::from_value(close.clone()).map_err(|e| format!("close rows: {e}"))?;
        close
            .candidates
            .sort_by(|a, b| a.prior.cmp(&b.prior).then_with(|| a.fresh.cmp(&b.fresh)));
        for p in &mut close.pairs {
            p.transfers.sort_by(|a, b| {
                cmp_utf16(&a.old_name, &b.old_name)
                    .then_with(|| cmp_utf16(&a.new_name, &b.new_name))
            });
            p.hints.sort_by(|a, b| cmp_utf16(&a.new_name, &b.new_name));
            p.snaps.sort_by(|a, b| cmp_utf16(&a.new_name, &b.new_name));
        }
        close
            .pairs
            .sort_by(|a, b| a.prior.cmp(&b.prior).then_with(|| a.fresh.cmp(&b.fresh)));
        w.json("matches-close.json", &close)?;
    }
    Ok(())
}

/// One LLM dispatch of the run, by site (`recordPromptDump`'s callers).
pub enum Dispatch<'a> {
    /// A naming wave's request (`site: "naming"`).
    Naming(&'a DispatchRecord),
    /// The coverage sweep's (`site: "sweep"`), with its anchored text.
    Sweep(Anchor, &'a SweepDispatch),
    /// A single-call namer: the vendor namer (`vendor`), the split's file
    /// namer and tree reviser (`folders`).
    Plain {
        function_id: &'static str,
        site: &'static str,
        call: &'a LlmCall,
    },
}

/// The run's dispatches in the TS's recording order: the vendor namer
/// (unpack), the naming waves, the sweeps (pre-generate, then the
/// deferred one), the split's namers.
fn run_dispatches<'a>(inp: &'a DumpInputs<'a>) -> Vec<Dispatch<'a>> {
    let out = inp.outcome;
    let mut d: Vec<Dispatch<'a>> = inp
        .vendor_prompts
        .iter()
        .map(|call| Dispatch::Plain {
            function_id: "vendor-namer",
            site: "vendor",
            call,
        })
        .collect();
    d.extend(out.waves.dispatches.iter().map(Dispatch::Naming));
    d.extend(out.pre_sweep.iter().flat_map(|s| {
        s.dispatches
            .iter()
            .map(|x| Dispatch::Sweep(Anchor::Fresh, x))
    }));
    d.extend(out.deferred_sweep.iter().flat_map(|(a, s)| {
        s.result
            .dispatches
            .iter()
            .map(move |x| Dispatch::Sweep(*a, x))
    }));
    if let Some(split) = inp.split {
        d.extend(split.prompts.iter().map(|(id, call)| Dispatch::Plain {
            function_id: id,
            site: "folders",
            call,
        }));
    }
    d
}

/// prompts.jsonl + cache-keys.jsonl (`recordPrompt` + `writePrompts` +
/// `writeCacheKeys`): one row per dispatch, `seq` in recording order,
/// `round` counted per functionId across every site.
pub fn dispatch_rows(dispatches: &[Dispatch<'_>], params: &CacheKeyParams) -> (String, String) {
    let mut prompts = String::new();
    let mut keys = String::new();
    let mut rounds: HashMap<String, u64> = HashMap::new();
    let num = |n: f64| JsValue::Number(n);
    for (seq, d) in dispatches.iter().enumerate() {
        let (function_id, site, request, system, user, cache_key) = match d {
            Dispatch::Naming(r) => (
                r.function_id.as_str(),
                "naming",
                &r.request,
                r.system_prompt.as_str(),
                r.user_prompt.as_str(),
                r.cache_key.clone(),
            ),
            Dispatch::Sweep(_, s) => (
                "coverage-sweep",
                "sweep",
                &s.request,
                s.system_prompt.as_str(),
                s.user_prompt.as_str(),
                s.cache_key.clone(),
            ),
            Dispatch::Plain {
                function_id,
                site,
                call,
            } => (
                *function_id,
                *site,
                &call.request,
                call.system_prompt.as_str(),
                call.user_prompt.as_str(),
                cache_key_of(&call.request, params),
            ),
        };
        let counted = rounds.entry(function_id.to_string()).or_insert(0);
        *counted += 1;
        // The waves count their own rounds (the processor's counter).
        let round = match d {
            Dispatch::Naming(r) => r.round,
            _ => *counted,
        };
        let target = |sid: &str, start: u32, end: u32, text: &str| {
            let mut t = JsObject::new();
            t.insert("sessionId", JsValue::str(sid));
            t.insert("start", num(f64::from(start)));
            t.insert("end", num(f64::from(end)));
            t.insert("text", JsValue::str(text));
            JsValue::Object(t)
        };
        let targets: Vec<JsValue> = match d {
            Dispatch::Naming(r) => r
                .targets
                .iter()
                .map(|(sid, s)| target(sid, s.start, s.end, "fresh"))
                .collect(),
            Dispatch::Sweep(a, s) => s
                .targets
                .iter()
                .map(|(name, sp)| target(name, sp.start, sp.end, a.as_str()))
                .collect(),
            Dispatch::Plain { .. } => Vec::new(),
        };
        let mut row = JsObject::new();
        row.insert("seq", num(seq as f64));
        row.insert("functionId", JsValue::str(function_id));
        row.insert("site", JsValue::str(site));
        row.insert("round", num(round as f64));
        if let Dispatch::Naming(r) = d {
            row.insert("wave", num(r.wave as f64));
        }
        row.insert("isRetry", JsValue::Bool(request.is_retry == Some(true)));
        row.insert("cacheKey", JsValue::str(cache_key.as_str()));
        row.insert("systemPrompt", JsValue::str(system));
        row.insert("userPrompt", JsValue::str(user));
        row.insert("identifiers", JsValue::str_array(&request.identifiers));
        row.insert("targets", JsValue::Array(targets));
        if let Dispatch::Sweep(a, _) = d {
            row.insert("targetsText", JsValue::str(a.as_str()));
        }
        prompts.push_str(&stringify(&JsValue::Object(row)));
        prompts.push('\n');
        let mut key = JsObject::new();
        key.insert("seq", num(seq as f64));
        key.insert("params", params_json(params));
        key.insert("request", request_material(request));
        key.insert("cacheKey", JsValue::str(cache_key.as_str()));
        keys.push_str(&stringify(&JsValue::Object(key)));
        keys.push('\n');
    }
    // `${lines.join("\n")}\n`: no rows is one newline.
    if dispatches.is_empty() {
        return ("\n".to_string(), "\n".to_string());
    }
    (prompts, keys)
}

/// `cacheKeyMaterialRow`'s request: the typed request flattened, Sets in
/// their actual order, the callee `snippet` DROPPED (the oracle dump's
/// shape — 16-findings #7), undefined fields absent; the TS object's keys
/// in its literal order.
fn request_material(r: &BatchRenameRequest) -> JsValue {
    let mut o = JsObject::new();
    o.insert("code", JsValue::str(r.code.as_str()));
    o.insert("identifiers", JsValue::str_array(&r.identifiers));
    o.insert("usedNames", JsValue::str_array(&r.used_names));
    o.insert(
        "calleeSignatures",
        JsValue::Array(
            r.callee_signatures
                .iter()
                .map(|c| {
                    let mut s = JsObject::new();
                    s.insert("name", JsValue::str(c.name.as_str()));
                    s.insert("params", JsValue::str_array(&c.params));
                    JsValue::Object(s)
                })
                .collect(),
        ),
    );
    o.insert("callsites", JsValue::str_array(&r.callsites));
    o.insert_opt(
        "contextVars",
        r.context_vars.as_deref().map(JsValue::str_array),
    );
    o.insert_opt(
        "priorVersionCode",
        r.prior_version_code.as_deref().map(JsValue::str),
    );
    o.insert_opt(
        "priorVersionNames",
        r.prior_version_names.as_deref().map(JsValue::str_array),
    );
    o.insert_opt(
        "priorNameHints",
        r.prior_name_hints.as_ref().map(StrMap::to_js),
    );
    o.insert_opt(
        "alreadyRenamed",
        r.already_renamed.as_ref().map(StrMap::to_js),
    );
    o.insert_opt("isRetry", r.is_retry.map(JsValue::Bool));
    o.insert_opt(
        "previousAttempt",
        r.previous_attempt.as_ref().map(StrMap::to_js),
    );
    o.insert_opt(
        "failures",
        r.failures.as_ref().map(|f| {
            let mut x = JsObject::new();
            x.insert("duplicates", JsValue::str_array(&f.duplicates));
            x.insert("invalid", JsValue::str_array(&f.invalid));
            x.insert("missing", JsValue::str_array(&f.missing));
            x.insert("unchanged", JsValue::str_array(&f.unchanged));
            JsValue::Object(x)
        }),
    );
    o.insert_opt("promptBody", r.prompt_body.as_deref().map(JsValue::str));
    o.insert_opt("userPrompt", r.user_prompt.as_deref().map(JsValue::str));
    o.insert_opt("systemPrompt", r.system_prompt.as_deref().map(JsValue::str));
    JsValue::Object(o)
}

/// `{ ...params }` (CacheKeyParams: model, temperature, maxTokens,
/// reasoningEffort; undefined absent).
fn params_json(p: &CacheKeyParams) -> JsValue {
    let mut o = JsObject::new();
    o.insert("model", JsValue::str(p.model.as_str()));
    o.insert_opt("temperature", p.temperature.map(JsValue::Number));
    o.insert_opt("maxTokens", p.max_tokens.map(|m| JsValue::Number(m as f64)));
    o.insert_opt(
        "reasoningEffort",
        p.reasoning_effort.as_deref().map(JsValue::str),
    );
    JsValue::Object(o)
}

/// tree-manifest.json (`treeManifest`): every file of the written tree,
/// each directory's entries in code-unit name order, `{path, sha256,
/// bytes}`.
fn tree_manifest(root: &Path) -> Result<String, String> {
    fn walk(dir: &Path, rel: &str, out: &mut Vec<JsValue>) -> Result<(), String> {
        let mut entries: Vec<std::fs::DirEntry> = std::fs::read_dir(dir)
            .map_err(|e| format!("read {}: {e}", dir.display()))?
            .collect::<Result<_, _>>()
            .map_err(|e| format!("read {}: {e}", dir.display()))?;
        entries.sort_by(|a, b| {
            cmp_utf16(
                &a.file_name().to_string_lossy(),
                &b.file_name().to_string_lossy(),
            )
        });
        for entry in entries {
            let name = entry.file_name().to_string_lossy().into_owned();
            let child_rel = if rel.is_empty() {
                name
            } else {
                format!("{rel}/{name}")
            };
            let path = entry.path();
            let is_dir = entry
                .file_type()
                .map_err(|e| format!("stat {}: {e}", path.display()))?
                .is_dir();
            if is_dir {
                walk(&path, &child_rel, out)?;
            } else {
                let bytes =
                    std::fs::read(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
                let sha: String = Sha256::digest(&bytes)
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect();
                let mut row = JsObject::new();
                row.insert("path", JsValue::str(child_rel));
                row.insert("sha256", JsValue::str(sha));
                row.insert("bytes", JsValue::Number(bytes.len() as f64));
                out.push(JsValue::Object(row));
            }
        }
        Ok(())
    }
    let mut files = Vec::new();
    walk(root, "", &mut files)?;
    let mut o = JsObject::new();
    o.insert("files", JsValue::Array(files));
    Ok(stringify(&JsValue::Object(o)))
}

/// regions.json (`writeRegions`): the library stage's owner
/// ([`crate::libdetect::function_carry::regions_json`]) over the processed
/// file's comment regions, the functions the library freeze froze, and
/// the graph-time Bun classification's factories (none on a real Bun
/// bundle: the beautifier splits the helper's marker).
fn write_regions(w: &Writer<'_>, inp: &DumpInputs<'_>, graph: Option<&Site>) -> Result<(), String> {
    let banners: Vec<JsValue> = graph
        .map(|(c, _)| {
            let mut factories: Vec<&crate::modules::FactoryRecord> = c.factories.iter().collect();
            factories.sort_by_key(|f| f.span.start);
            factories
                .into_iter()
                .map(|f| {
                    let mut span = JsObject::new();
                    span.insert("start", JsValue::Number(f64::from(f.span.start)));
                    span.insert("end", JsValue::Number(f64::from(f.span.end)));
                    let mut o = JsObject::new();
                    o.insert("span", JsValue::Object(span));
                    o.insert("factoryVar", JsValue::str(f.factory_var.as_str()));
                    o.insert("structuralHash", JsValue::str(f.structural_hash.as_str()));
                    JsValue::Object(o)
                })
                .collect()
        })
        .unwrap_or_default();
    let regions = crate::libdetect::function_carry::regions_json(
        inp.comment_regions,
        &inp.outcome.library_functions,
        banners,
    );
    w.text("regions.json", &stringify(&regions))
}

#[cfg(test)]
mod artifact_dump_test;
