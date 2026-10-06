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
//! twin-gates.json / private-renames.json (prior only), partitions.json,
//! matches.json, matches-close.json (when the close tier ran),
//! transfers.json, transfers-mechanical.json, votes.json, prompts.jsonl,
//! cache-keys.jsonl, names.json, placement.json, emit.json,
//! tree-manifest.json, regions.json.
//!
//! Separately, [`DispatchLog`] is the run's per-dispatch recorder
//! (finding #65): every dispatch site hands its row here the moment the
//! dispatch is committed. `--dump-artifacts` streams prompts.jsonl +
//! cache-keys.jsonl rows to part files as dispatches commit (no per-ask
//! string survives its row); `--dump-asks` retains the small ask rows for
//! the end-of-run log; a run with NEITHER flag retains nothing per ask —
//! the dispatch records the run used to accumulate for the whole run were
//! the ~99GB holder of a full-bundle fresh run.

use std::collections::HashMap;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use humanify_model::dump::{
    EmitLayoutFile, FunctionsFile, MatchPair, MatchRejection, MatchesCloseFile, NamesFile,
    PartitionFamily, PartitionMember, PartitionsFile, SpanKey, TransfersFile, VotesFile,
};
use humanify_model::js::{JsObject, JsValue, cmp_utf16, stringify};
use humanify_model::llm::{BatchRenameRequest, CacheKeyParams, LlmCall, StrMap, cache_key_of};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::naming::ask_trace::{AskScope, AskSite};
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
    /// The processed file's library comment regions (its mixed-file
    /// detection), MINIFIED-text byte offsets.
    pub comment_regions: &'a [crate::libdetect::CommentRegion],
    /// The post-split reconcile's trail rows, per split file (their
    /// spans index the file's text).
    pub extra_trail: &'a crate::naming::report::diagnostics::ExtraTrail,
    /// The run's bundle layout (the classification sites' container).
    pub layout: crate::toolchain::BundleLayout,
    /// The run's module wrapper grammar (the classification sites').
    pub module_wrappers: crate::toolchain::ModuleWrapperGrammar,
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
    let graph_site =
        crate::modules::modules_dump::classify_site(inp.fresh, inp.layout, inp.module_wrappers)
            .map_err(|e| format!("fresh: {e}"))?;
    write_modules(&w, inp, graph_site.as_ref())?;
    if let Some(m) = capture.and_then(|c| c.matches.as_ref()) {
        write_twin_gates(&w, &m.twin_gates)?;
        write_twins(&w, &m.twins)?;
        w.json(
            "private-renames.json",
            &crate::matching::matches_dump::private_renames_file(&m.private_renames),
        )?;
    }
    write_partitions(&w, inp)?;
    write_matches(&w, inp)?;
    for x in inp.extra_trail {
        w.text(&format!("text/post-split/{}", x.file), &x.text)?;
    }
    let extra = extra_keys(inp.extra_trail);
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
    // prompts.jsonl + cache-keys.jsonl are NOT written here: their rows
    // were streamed by the run's `DispatchLog` as dispatches committed
    // (finding #65) and assembled into this dir at the log's close.
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

/// The post-split reconcile's rows keyed in their own split file (finding
/// #50): the file's tree-relative path (07 §1's path key space) and the
/// row's UTF-8 byte span in the text the pass parsed — the dump writes
/// that text to `text/post-split/<path>`.
fn extra_keys(
    extra: &crate::naming::report::diagnostics::ExtraTrail,
) -> Vec<crate::naming::driver::dump::ExtraNameRow<'_>> {
    let mut out = Vec::new();
    for x in extra {
        let lines = crate::babel_view::BabelLines::new(&x.text);
        for e in &x.rows {
            let (start, end) = (e.target.decl_span.start, e.target.decl_span.end);
            let (line, col) = lines.loc(start);
            out.push(crate::naming::driver::dump::ExtraNameRow {
                key: span_key(&x.file, start, end),
                loc: format!("{line}:{col}"),
                entry: e,
            });
        }
    }
    out
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
/// classification, the Rust's own factory hashes — WP5.6e) and the graph site (the fresh text's — None on a real Bun
/// bundle: the beautifier splits the `{exports:{}}` marker). No file when
/// neither site classified.
fn write_modules(w: &Writer<'_>, inp: &DumpInputs<'_>, graph: Option<&Site>) -> Result<(), String> {
    use crate::modules::modules_dump::{classify_site, site_json};
    let unpack = classify_site(inp.minified, inp.layout, inp.module_wrappers)
        .map_err(|e| format!("minified: {e}"))?;
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

/// What each dispatch site does with a committed dispatch (finding #65).
/// The modes are the CLI flags: `Off` is a run with neither dump flag —
/// nothing per ask survives the dispatch; `Asks` is `--dump-asks` — the
/// small ask row is retained for the end-of-run log, never the prompt
/// text; `Full` is `--dump-artifacts` (and the test log, which retains
/// the records) — the prompt and key rows are written AT COMMIT, so no
/// per-ask string is held past its row either.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RecordMode {
    Off,
    Asks,
    Full,
}

/// The run's per-dispatch recorder. Every site hands its dispatch here the
/// moment it is committed; row order is commit order, which is the dump's
/// own concatenation order (the vendor namer, the naming sites, the
/// split's namers — the stages never interleave). The waves' pipelined
/// rounds are the one deferral: their records commit at the round's
/// canonical sort, so a round's `Full` records live until then (a round,
/// never the run).
pub struct DispatchLog {
    params: CacheKeyParams,
    /// `--dump-asks`: the ask rows, in commit order (vendor rows first —
    /// [`Self::begin_named_file`] keeps only those when a new file starts).
    asks: Option<Vec<JsValue>>,
    vendor_asks: Option<usize>,
    /// `--dump-artifacts`: the two JSONL part files. None keeps no rows.
    rows: Option<LogRows>,
    /// Tests read the dispatches back out of the sites' own vectors; the
    /// production modes never retain one.
    retain: bool,
    seq: u64,
    rounds: HashMap<String, u64>,
}

impl DispatchLog {
    /// A run with neither dump flag: every [`Self::record`] is a no-op and
    /// no per-ask memory is retained anywhere.
    pub fn off(params: CacheKeyParams) -> Self {
        DispatchLog {
            params,
            asks: None,
            vendor_asks: None,
            rows: None,
            retain: false,
            seq: 0,
            rounds: HashMap::new(),
        }
    }

    /// `--dump-asks`: the ask rows only.
    pub fn asks(params: CacheKeyParams) -> Self {
        let mut log = Self::off(params);
        log.asks = Some(Vec::new());
        log
    }

    /// `--dump-artifacts`: the part files opened in `dir` now (the dump's
    /// other files are still written once, at the end).
    pub fn dump(params: CacheKeyParams, dir: &Path) -> std::io::Result<Self> {
        std::fs::create_dir_all(dir)?;
        let rows = LogRows::open(dir)?;
        let mut log = Self::off(params);
        log.asks = Some(Vec::new());
        log.rows = Some(rows);
        Ok(log)
    }

    /// The name of the site a dispatch belongs to (its dump row's `site`).
    fn site_of(d: &Dispatch<'_>) -> &'static str {
        match d {
            Dispatch::Naming(_) => "naming",
            Dispatch::Sweep(_, _) => "sweep",
            Dispatch::Plain { site, .. } => site,
        }
    }

    /// The mode the dispatch sites prepare their records for.
    pub fn mode(&self) -> RecordMode {
        if self.rows.is_some() || self.retain {
            RecordMode::Full
        } else if self.asks.is_some() {
            RecordMode::Asks
        } else {
            RecordMode::Off
        }
    }

    /// Whether the sites retain the dispatches in their own vectors (the
    /// tests' log — production modes never do).
    pub fn retains(&self) -> bool {
        self.retain
    }

    /// The tests' log: full records retained by the sites, rows collected
    /// in memory as the byte-identity oracle.
    pub fn retain_for_tests(params: CacheKeyParams) -> Self {
        let mut log = Self::asks(params);
        log.retain = true;
        log.rows = Some(LogRows::in_memory());
        log
    }

    fn wants_rows(&self) -> bool {
        self.rows.is_some()
    }

    /// Record one committed dispatch: its prompt/key row (the dump) and/or
    /// its ask row (the ask log), in commit order.
    pub fn record(&mut self, d: &Dispatch<'_>) {
        if self.mode() == RecordMode::Off {
            return;
        }
        let round = round_of(d, &mut self.rounds);
        let seq = self.next_seq();
        if self.asks.is_some() {
            let parts = ask_parts_rounded(d, round);
            if let Some(asks) = &mut self.asks {
                asks.push(ask_row(seq, &parts));
            }
        }
        if self.wants_rows() {
            let (prompt, key) = prompt_rows(seq, round, d, &self.params);
            let site = Self::site_of(d);
            if let Some(rows) = &mut self.rows {
                rows.write(site, &prompt, &key);
            }
        }
    }

    /// Record one committed NAMING dispatch whose material the waves could
    /// not retain (the `Asks` mode's slim round records — never the prompt
    /// text): its ask row alone.
    pub fn record_ask_parts(&mut self, parts: &AskRowParts) {
        if self.asks.is_some() {
            let seq = self.next_seq();
            if let Some(asks) = &mut self.asks {
                asks.push(ask_row(seq, parts));
            }
        }
    }

    /// The TS dump's LAST-FILE rule: a run that names several files dumps
    /// only the last one's naming rows, so each file's naming section
    /// starts from the vendor rows' seq again. The vendor rows themselves
    /// (written before any file is named) are kept.
    pub fn begin_named_file(&mut self) {
        if self.mode() == RecordMode::Off {
            return;
        }
        if let Some(rows) = &mut self.rows {
            rows.begin_named_file();
        }
        let base = match self.vendor_asks {
            Some(len) => len,
            None => {
                let len = self.asks.as_ref().map_or(0, Vec::len);
                self.vendor_asks = Some(len);
                len
            }
        };
        if let Some(asks) = &mut self.asks {
            asks.truncate(base);
        }
        self.seq = base as u64;
    }

    /// The ask rows so far (the `--dump-asks` log's content).
    pub fn ask_rows(&self) -> &[JsValue] {
        self.asks.as_deref().unwrap_or(&[])
    }

    /// The tests' in-memory rows: (prompts.jsonl, cache-keys.jsonl) per
    /// dispatch, in commit order.
    pub fn memory_rows(&self) -> &[(String, String)] {
        self.rows.as_ref().map_or(&[][..], |r| r.memory())
    }

    fn next_seq(&mut self) -> u64 {
        let seq = self.seq;
        self.seq += 1;
        seq
    }

    /// Assemble the dump's prompts.jsonl + cache-keys.jsonl from their
    /// parts (kept even when the run wrote zero rows: both files hold one
    /// newline, `writePrompts`' empty case). An error here is the same
    /// error `write_artifact_dump` would have reported. Idempotent.
    pub fn close(&mut self) -> Result<(), String> {
        match self.rows.take() {
            Some(rows) => rows.close(),
            None => Ok(()),
        }
    }

    /// Drop the part files without assembling them — the failed-run rule:
    /// a run whose dump was never written leaves no dump files behind
    /// (exactly the TS behavior).
    pub fn discard(&mut self) {
        if let Some(rows) = self.rows.take() {
            rows.discard();
        }
    }
}

/// Where the `Full` mode's prompt/key rows go: the dump's part files
/// (`--dump-artifacts`) or, for the tests' log, memory (`vendor_len` =
/// the same vendor/naming split the part files make).
enum LogRows {
    Files(LogFiles),
    Memory {
        rows: Vec<(String, String)>,
        /// Where the vendor rows end (frozen at the first named file) —
        /// the same vendor/naming split the part files make.
        vendor_base: Option<usize>,
    },
}

impl LogRows {
    fn open(dir: &Path) -> std::io::Result<Self> {
        Ok(LogRows::Files(LogFiles::open(dir)?))
    }

    fn in_memory() -> Self {
        LogRows::Memory {
            rows: Vec::new(),
            vendor_base: None,
        }
    }

    fn write(&mut self, site: &str, prompt: &str, key: &str) {
        match self {
            LogRows::Files(files) => files.write(site, prompt, key),
            LogRows::Memory { rows, .. } => {
                rows.push((prompt.to_string(), key.to_string()));
            }
        }
    }

    fn begin_named_file(&mut self) {
        match self {
            LogRows::Files(files) => files.begin_named_file(),
            LogRows::Memory { rows, vendor_base } => match vendor_base {
                Some(len) => rows.truncate(*len),
                None => *vendor_base = Some(rows.len()),
            },
        }
    }

    fn memory(&self) -> &[(String, String)] {
        match self {
            LogRows::Files(_) => &[],
            LogRows::Memory { rows, .. } => rows,
        }
    }

    fn close(self) -> Result<(), String> {
        match self {
            LogRows::Files(files) => files.close(),
            LogRows::Memory { .. } => Ok(()),
        }
    }

    fn discard(self) {
        if let LogRows::Files(files) = self {
            files.discard();
        }
    }
}

/// The dump's two JSONL files, as two parts each: the vendor rows (every
/// row that precedes the named files) and the current file's naming rows
/// — truncated by [`DispatchLog::begin_named_file`], so a multi-file run
/// dumps the last file's rows exactly as the TS's last-file dump did.
/// [`LogFiles::close`] concatenates the parts into prompts.jsonl and
/// cache-keys.jsonl.
struct LogFiles {
    dir: PathBuf,
    vendor: Pair,
    /// None once a re-create failed (the run keeps going; the error
    /// surfaces at close, like any dump write error).
    naming: Option<Pair>,
    rows: u64,
    error: Option<String>,
}

/// One part pair: the prompts file and the cache-keys file.
struct Pair {
    prompts: BufWriter<std::fs::File>,
    keys: BufWriter<std::fs::File>,
}

impl Pair {
    fn create(dir: &Path, suffix: &str) -> std::io::Result<Pair> {
        let prompts = BufWriter::new(std::fs::File::create(
            dir.join(format!("prompts.jsonl{suffix}")),
        )?);
        let keys = BufWriter::new(std::fs::File::create(
            dir.join(format!("cache-keys.jsonl{suffix}")),
        )?);
        Ok(Pair { prompts, keys })
    }

    fn write(&mut self, prompt: &str, key: &str) {
        let _ = self.prompts.write_all(prompt.as_bytes());
        let _ = self.prompts.write_all(b"\n");
        let _ = self.keys.write_all(key.as_bytes());
        let _ = self.keys.write_all(b"\n");
    }

    fn flush(&mut self, what: &str) -> Result<(), String> {
        self.prompts
            .flush()
            .map_err(|e| format!("write prompts.jsonl{what}: {e}"))?;
        self.keys
            .flush()
            .map_err(|e| format!("write cache-keys.jsonl{what}: {e}"))?;
        Ok(())
    }
}

impl LogFiles {
    fn open(dir: &Path) -> std::io::Result<LogFiles> {
        Ok(LogFiles {
            dir: dir.to_path_buf(),
            vendor: Pair::create(dir, ".vendorpart")?,
            naming: Some(Pair::create(dir, ".namingpart")?),
            rows: 0,
            error: None,
        })
    }

    fn write(&mut self, site: &str, prompt: &str, key: &str) {
        match site {
            "vendor" => self.vendor.write(prompt, key),
            _ => {
                if let Some(naming) = self.naming.as_mut() {
                    naming.write(prompt, key);
                }
            }
        }
        self.rows += 1;
    }

    fn begin_named_file(&mut self) {
        if self.error.is_some() {
            return;
        }
        self.naming = match Pair::create(&self.dir, ".namingpart") {
            Ok(pair) => Some(pair),
            Err(e) => {
                self.error = Some(e.to_string());
                None
            }
        };
    }

    /// Remove the part files, no assembly.
    fn discard(self) {
        let LogFiles { dir, .. } = self;
        for name in ["prompts", "cache-keys"] {
            let _ = std::fs::remove_file(dir.join(format!("{name}.jsonl.vendorpart")));
            let _ = std::fs::remove_file(dir.join(format!("{name}.jsonl.namingpart")));
        }
    }

    fn close(self) -> Result<(), String> {
        if let Some(e) = self.error {
            return Err(e);
        }
        let LogFiles {
            dir,
            mut vendor,
            naming,
            rows,
            ..
        } = self;
        let mut naming = naming.ok_or("the naming part writer failed mid-run")?;
        vendor.flush(".vendorpart")?;
        vendor.flush(".vendorpart")?;
        naming.flush(".namingpart")?;
        drop(vendor);
        drop(naming);
        if rows == 0 {
            std::fs::write(dir.join("prompts.jsonl"), "\n")
                .map_err(|e| format!("write prompts.jsonl: {e}"))?;
            std::fs::write(dir.join("cache-keys.jsonl"), "\n")
                .map_err(|e| format!("write cache-keys.jsonl: {e}"))?;
        } else {
            concat(&dir, "prompts.jsonl")?;
            concat(&dir, "cache-keys.jsonl")?;
        }
        let _ = std::fs::remove_file(dir.join("prompts.jsonl.vendorpart"));
        let _ = std::fs::remove_file(dir.join("cache-keys.jsonl.vendorpart"));
        let _ = std::fs::remove_file(dir.join("prompts.jsonl.namingpart"));
        let _ = std::fs::remove_file(dir.join("cache-keys.jsonl.namingpart"));
        Ok(())
    }
}

/// `final = .vendorpart ++ .namingpart`, streamed (the parts are large).
fn concat(dir: &Path, name: &str) -> Result<(), String> {
    use std::io::copy;
    let mut out =
        std::fs::File::create(dir.join(name)).map_err(|e| format!("write {name}: {e}"))?;
    for part in [".vendorpart", ".namingpart"] {
        let mut r = std::fs::File::open(dir.join(format!("{name}{part}")))
            .map_err(|e| format!("read {name}{part}: {e}"))?;
        copy(&mut r, &mut out).map_err(|e| format!("assemble {name}: {e}"))?;
    }
    Ok(())
}

/// Write the reason-labeled ask log (`--dump-asks`): the ask rows the run
/// recorded, in commit order. Recording only — no decision reads it, and
/// no row exists unless the flag asks for it. Returns the row count.
pub fn write_ask_rows(path: &Path, rows: &[JsValue]) -> Result<usize, String> {
    let mut text = String::new();
    for row in rows {
        text.push_str(&stringify(row));
        text.push('\n');
    }
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).map_err(|e| format!("mkdir {}: {e}", parent.display()))?;
    }
    std::fs::write(path, text).map_err(|e| format!("write {}: {e}", path.display()))?;
    Ok(rows.len())
}

/// One ask-log row's material — small enough to retain for a whole run:
/// WHY the ask happened (`naming::ask_trace`'s taxonomy), what it asked,
/// and the context it carried. Never the prompt text, never the model's
/// answer — the `Asks` mode builds this at PREPARE time so an ask is
/// recorded without ever materializing the prompt (finding #65).
pub struct AskRowParts {
    site: &'static str,
    scope: String,
    scope_kind: &'static str,
    reason: crate::naming::ask_trace::AskReason,
    is_retry: bool,
    /// A recorded cause wins (an applier-rejection seed, or a lane round-2
    /// seeded by the scope check — `AskSite::lane_reask`); otherwise a
    /// lane's round-2 is derived from the request's failure lists.
    retry_cause: Option<crate::naming::ask_trace::RetryCause>,
    retry_cause_detail: Option<String>,
    prior: bool,
    wave: Option<u64>,
    phase: u8,
    round: u64,
    identifiers: Vec<String>,
    used_names_count: usize,
    variant: &'static str,
}

impl AskRowParts {
    /// The row's fields, resolved one dispatch at a time.
    pub fn of(
        site: &'static str,
        kind: crate::naming::ask_trace::AskScope,
        scope: String,
        wave: Option<u64>,
        round: u64,
        ask: &AskSite,
        request: &BatchRenameRequest,
    ) -> AskRowParts {
        use crate::naming::ask_trace::{prior_context_of, reason_of};
        let is_retry = request.is_retry == Some(true);
        let reason = reason_of(ask, kind, request);
        let retry_cause = ask.cause.or_else(|| {
            if is_retry {
                request
                    .failures
                    .as_ref()
                    .and_then(crate::naming::ask_trace::RetryCause::of_failures)
            } else {
                None
            }
        });
        let scope_kind = match kind {
            AskScope::Fn => "fn",
            AskScope::Module => "module",
            AskScope::Sweep => "sweep",
            AskScope::Vendor => "vendor",
            AskScope::Folders => "folders",
            AskScope::Modules => "modules",
        };
        let variant = match (kind, is_retry) {
            (AskScope::Fn, false) => "batch",
            (AskScope::Fn, true) => "batch-retry",
            (AskScope::Module, false) => "module",
            (AskScope::Module, true) => "module-retry",
            (AskScope::Sweep, false) => "batch",
            (AskScope::Sweep, true) => "batch-retry",
            (AskScope::Vendor, _) => "vendor",
            (AskScope::Folders, _) => "folders",
            (AskScope::Modules, _) => "modules",
        };
        AskRowParts {
            site,
            scope,
            scope_kind,
            reason,
            is_retry,
            retry_cause,
            retry_cause_detail: ask.detail.clone(),
            prior: ask.prior || prior_context_of(request),
            wave,
            phase: ask.phase,
            round,
            identifiers: request.identifiers.clone(),
            used_names_count: request.used_names.len(),
            variant,
        }
    }

    /// The scope the round counter counts (the ask row's `scope`).
    pub fn scope(&self) -> &str {
        self.scope.as_str()
    }

    /// The round, assigned at commit (the functionId's call count so far).
    pub fn set_round(&mut self, round: u64) {
        self.round = round;
    }
}

/// One ask-log row, the TS writer's keys in order.
pub fn ask_row(seq: u64, parts: &AskRowParts) -> JsValue {
    let num = |n: f64| JsValue::Number(n);
    let mut row = JsObject::new();
    row.insert("seq", num(seq as f64));
    row.insert("site", JsValue::str(parts.site));
    row.insert("scope", JsValue::str(parts.scope.as_str()));
    row.insert("scopeKind", JsValue::str(parts.scope_kind));
    row.insert("reason", JsValue::str(parts.reason.as_str()));
    row.insert("isRetry", JsValue::Bool(parts.is_retry));
    if let Some(cause) = parts.retry_cause {
        row.insert("retryCause", JsValue::str(cause.as_str()));
    }
    if let Some(detail) = &parts.retry_cause_detail {
        row.insert("retryCauseDetail", JsValue::str(detail));
    }
    row.insert("priorContext", JsValue::Bool(parts.prior));
    if let Some(wave) = parts.wave {
        row.insert("wave", num(wave as f64));
        row.insert("phase", num(parts.phase as f64));
    }
    row.insert("round", num(parts.round as f64));
    row.insert("identifiers", JsValue::str_array(&parts.identifiers));
    row.insert("usedNamesCount", num(parts.used_names_count as f64));
    row.insert("promptVariant", JsValue::str(parts.variant));
    JsValue::Object(row)
}

/// One ask-log row per dispatch, `seq` in recording order: WHY the ask
/// happened ([`crate::naming::ask_trace`]'s taxonomy), what it asked, and
/// the context it carried — never the model's answer. The bulk form (the
/// tests' oracle); a run records its rows through [`DispatchLog::record`].
pub fn ask_rows(dispatches: &[Dispatch<'_>]) -> Vec<JsValue> {
    let mut rounds: HashMap<String, u64> = HashMap::new();
    dispatches
        .iter()
        .enumerate()
        .map(|(seq, d)| {
            let round = round_of(d, &mut rounds);
            let parts = ask_parts_rounded(d, round);
            ask_row(seq as u64, &parts)
        })
        .collect()
}

/// [`AskRowParts::of`] from any committed dispatch, `round` already
/// counted by [`round_of`] (the waves carry their own; every other site
/// counts rounds per functionId).
fn ask_parts_rounded(d: &Dispatch<'_>, round: u64) -> AskRowParts {
    use crate::naming::ask_trace::AskScope;
    match d {
        Dispatch::Naming(r) => {
            let kind = if r.function_id.starts_with("module-binding-batch:") {
                AskScope::Module
            } else {
                AskScope::Fn
            };
            AskRowParts::of(
                "naming",
                kind,
                r.function_id.clone(),
                Some(r.wave),
                r.round,
                &r.ask,
                &r.request,
            )
        }
        Dispatch::Sweep(_, s) => AskRowParts::of(
            "sweep",
            AskScope::Sweep,
            "coverage-sweep".to_string(),
            None,
            round,
            &s.ask,
            &s.request,
        ),
        Dispatch::Plain {
            function_id,
            site,
            call,
        } => AskRowParts::of(
            site,
            match *site {
                "folders" => AskScope::Folders,
                crate::place::assign::namer::MODULE_NAMER_SITE => AskScope::Modules,
                _ => AskScope::Vendor,
            },
            function_id.to_string(),
            None,
            round,
            &AskSite::fresh(0),
            &call.request,
        ),
    }
}

/// The dump's per-functionId round counter (a sweep's / a namer's rounds
/// are their call count; the waves carry their own).
fn rounds_of(rounds: &mut HashMap<String, u64>, function_id: &str) -> u64 {
    let counted = rounds.entry(function_id.to_string()).or_insert(0);
    *counted += 1;
    *counted
}

/// One dispatch's dump round: the waves carry their own (the processor's
/// counter); every other site counts per functionId.
fn round_of(d: &Dispatch<'_>, rounds: &mut HashMap<String, u64>) -> u64 {
    match d {
        Dispatch::Naming(r) => r.round,
        Dispatch::Sweep(..) => rounds_of(rounds, "coverage-sweep"),
        Dispatch::Plain { function_id, .. } => rounds_of(rounds, function_id),
    }
}

/// One dispatch's prompts.jsonl row and cache-keys.jsonl row
/// (`recordPrompt`, `writePrompts`, `writeCacheKeys`): the row the `Full`
/// mode streams at commit ([`DispatchLog::record`]) — `seq` is its
/// recording index, `round` counted per functionId across every site.
pub fn prompt_rows(
    seq: u64,
    round: u64,
    d: &Dispatch<'_>,
    params: &CacheKeyParams,
) -> (String, String) {
    let num = |n: f64| JsValue::Number(n);
    {
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
        let prompt = stringify(&JsValue::Object(row));
        let mut key = JsObject::new();
        key.insert("seq", num(seq as f64));
        key.insert("params", params_json(params));
        key.insert("request", request_material(request));
        key.insert("cacheKey", JsValue::str(cache_key.as_str()));
        let key = stringify(&JsValue::Object(key));
        (prompt, key)
    }
}

/// prompts.jsonl + cache-keys.jsonl, every row in recording order — the
/// bulk form (the tests' byte-identity oracle; a run streams its rows
/// through [`DispatchLog::record`]). `${lines.join("\n")}\n`: no rows is
/// one newline.
pub fn dispatch_rows(dispatches: &[Dispatch<'_>], params: &CacheKeyParams) -> (String, String) {
    let mut prompts = String::new();
    let mut keys = String::new();
    let mut rounds: HashMap<String, u64> = HashMap::new();
    for (seq, d) in dispatches.iter().enumerate() {
        let (prompt, key) = prompt_rows(seq as u64, round_of(d, &mut rounds), d, params);
        prompts.push_str(&prompt);
        prompts.push('\n');
        keys.push_str(&key);
        keys.push('\n');
    }
    if dispatches.is_empty() {
        return ("\n".to_string(), "\n".to_string());
    }
    (prompts, keys)
}

/// `cacheKeyMaterialRow`'s request: the typed request flattened, Sets in
/// their actual order, undefined fields absent; the TS object's keys in
/// its literal order. The callee `snippet` IS carried (16-findings #7,
/// fixed 2026-09-29): the TS dump dropped it even though the cache key
/// hashes it, so 30–50% of rows per pair could not be re-derived from the
/// dump — the row now matches the key material exactly.
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
                    s.insert_opt("snippet", c.snippet.as_deref().map(JsValue::str));
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
    o.insert_opt(
        "priorRejects",
        r.prior_rejects
            .as_ref()
            .map(humanify_model::llm::PriorRejects::to_js),
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
