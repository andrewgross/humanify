//! Vendor naming (WPB.2-adjacent — the unpack adapter's naming + the
//! manifest write). TS originals: `src/unpack/adapters/bun.ts` (the
//! file-name derivation + the manifest assembly + the prior-manifest
//! loaders), `src/shared/cjs-factory.ts` (the filename floor), `src/
//! shared/unique-name.ts` (case-folding uniquify), `src/unpack/manifest-
//! order.ts` (annotateHashOrdinals + orderByPriorManifest), `src/unpack/
//! vendor-namer.ts` (the LLM fallback pass).
//!
//! The DETERMINISTIC cascade itself (banner → url → carry-over → fallback)
//! already lives in the parent (`super::name_cjs_factories`); this module
//! starts where it ends: the record carries `name`/`name_source` and this
//! file derives the on-disk file name, assembles the manifest, and runs the
//! post-cascade LLM pass over the hash-named leftovers.
//!
//! The LLM boundary is a trait (`VendorNamer`): the core pass is pure
//! orchestration and never touches a network. `ProviderVendorNamer` adapts
//! any `humanify_model::llm::NameProvider`; the CLI wires humanify-llm's
//! replay-only client over the LLM cache the oracle runs wrote (the key is
//! `humanify_model::llm::cache_key_of`, the cache I/O humanify-llm's — one
//! owner each, WP4.1). WITHOUT a namer
//! the pass is skipped — the TS behaves the same way (the pass only runs
//! when `options?.vendorNamer`) — so parity then only holds for runs WITH
//! the namer; with the cache-replay wiring the names are the ones the
//! oracle's run got, because the cache replays its answers.

use std::collections::{BTreeMap, HashMap, HashSet};

use serde::Serialize;
use sha2::{Digest, Sha256};

use humanify_model::llm::{BatchRenameRequest, LlmCall, NameProvider};

use super::{FactoryRecord, NameSource, hash_fallback_name, is_hash_fallback_name};

// ---------------------------------------------------------------------------
// The filename floor (shared/cjs-factory.ts)
// ---------------------------------------------------------------------------

/// A binding fit to become a vendor filename: at least 3 identifier chars
/// (`isVendorWorthyBinding` — `/^[A-Za-z_$][A-Za-z0-9_$]{2,}$/`). Anything
/// shorter is minified residue (H, qA) that must never name a file.
pub fn is_vendor_worthy_binding(name: &str) -> bool {
    let bytes = name.as_bytes();
    let Some((&first, rest)) = bytes.split_first() else {
        return false;
    };
    let head_ok = first.is_ascii_alphabetic() || first == b'_' || first == b'$';
    let tail_ok = rest.len() >= 2
        && rest
            .iter()
            .all(|&b| b.is_ascii_alphanumeric() || b == b'_' || b == b'$');
    head_ok && tail_ok
}

/// Drop a trailing ".js" from a package/file stem (case-insensitive — the
/// TS is `/\.js$/i`), so appending the real extension can never yield
/// highlight.js.js.
pub fn strip_js_extension(name: &str) -> String {
    if name.len() >= 3 {
        let (head, tail) = name.split_at(name.len() - 3);
        if tail.eq_ignore_ascii_case(".js") {
            return head.to_string();
        }
    }
    name.to_string()
}

/// Vendor file stem from an UNTRUSTED candidate (a binding or raw factory
/// var): trailing ".js" stripped, and minified residue floored to
/// `lib_<sha256(bodyText)[:8]>` — the same fallback family the naming
/// cascade uses.
pub fn vendor_stem_for(candidate: &str, body_text: &str) -> String {
    let stem = strip_js_extension(candidate);
    if is_vendor_worthy_binding(&stem) {
        return stem;
    }
    let digest = Sha256::digest(body_text.as_bytes());
    let hash: String = digest[..4].iter().map(|b| format!("{b:02x}")).collect();
    format!("lib_{hash}")
}

// ---------------------------------------------------------------------------
// File-name sanitization (adapters/bun.ts)
// ---------------------------------------------------------------------------

/// Sanitize a cascade name into something safe to use as a filename.
/// Keeps alphanumerics, `@`, `-`, `_`, `.`; replaces everything else. `/`
/// is common in scoped packages — converted to `__`.
pub fn sanitize_fs_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        if c == '/' {
            out.push_str("__");
        } else if c == '@' || c == '-' || c == '_' || c == '.' || c.is_ascii_alphanumeric() {
            out.push(c);
        } else {
            out.push('_');
        }
    }
    out
}

/// Like sanitize_fs_name but KEEPS `/` as a path separator, sanitizing each
/// segment — so an @scope/name package becomes a nested folder
/// (vendor/@scope/name/…), the way node_modules lays scoped packages out.
/// Empty segments drop (the TS `.filter(Boolean)`).
pub fn sanitize_fs_path(name: &str) -> String {
    name.split('/')
        .map(sanitize_fs_name)
        .filter(|seg| !seg.is_empty())
        .collect::<Vec<_>>()
        .join("/")
}

/// Case-folding name disambiguation (shared/unique-name.ts): `stem` made
/// unique among the names already in `used` UNDER CASE-FOLDING, appending
/// `-2`, `-3`, … on a collision. A case-INSENSITIVE filesystem collapses
/// two names that differ only in case, so the single source of truth for
/// "make this name unique" must fold case.
pub fn unique_case_insensitive_name(stem: &str, used: &mut HashSet<String>) -> String {
    let mut name = stem.to_string();
    let mut k = 2;
    while used.contains(&name.to_lowercase()) {
        name = format!("{stem}-{k}");
        k += 1;
    }
    used.insert(name.to_lowercase());
    name
}

// ---------------------------------------------------------------------------
// Naming lookup (adapters/bun.ts chooseFileName, over the classified
// records in bundle order)
// ---------------------------------------------------------------------------

/// The resolved file naming for one factory (`NameLookup`).
pub struct NameLookup {
    /// Path of the file relative to the output root, WITHOUT the `.js`
    /// extension (the TS plan.naming.fileName — the caller appends `.js`
    /// and the `vendor/` prefix).
    pub file_name: String,
    pub name: String,
    pub name_source: NameSource,
    pub structural_hash: String,
}

/// The module's cross-version-stable identity stem: `lib_<structuralHash8>`
/// when classified, else the content-floored factory var (`stableStem`).
/// Used for the free identifier so it survives display-name and folder
/// changes.
pub fn stable_stem(record: &FactoryRecord) -> String {
    if record.structural_hash.is_empty() {
        record.factory_var.clone()
    } else {
        hash_fallback_name(&record.structural_hash)
    }
}

/// Derive the on-disk file name for every classified factory, in bundle
/// order — `FileNameChooser` over every record (the AST path, where every
/// classified factory is extracted).
pub fn choose_file_names(factories: &[FactoryRecord]) -> Vec<NameLookup> {
    let mut chooser = FileNameChooser::new(factories.iter());
    factories
        .iter()
        .map(|record| chooser.choose(&record.factory_var, Some(record), ""))
        .collect()
}

/// The adapter's per-run file-name state (`planModules`' `usedByFolder` +
/// `nameCounts`) and `chooseFileName` over it, one module at a time.
///
/// A package that identified >=2 modules groups into `vendor/<package>/`,
/// each module named by its stable structural stem; a single-identifying
/// package stays flat, named by its sanitized cascade name. The grouping
/// gate is on the NAME, not nameSource: a lib_<hash> fallback identifies no
/// package, and the same name carried from a prior release arrives as
/// carry-over, so a source test would group on the second hop what it left
/// flat on the first.
pub struct FileNameChooser {
    /// Exact-name occurrence counts over identified (non-fallback) records
    /// (`countIdentifiedNames`) — a name shared by >=2 is a package with
    /// multiple internal modules. Exact, not case-folded: two packages
    /// differing only in case are distinct libraries, not one folder.
    name_counts: HashMap<String, usize>,
    /// Per-folder lowercased used stems — uniquify folds case (a
    /// case-insensitive FS collapses Foo.js and foo.js). The root folder is
    /// keyed "". (`usedByFolder`.)
    used_by_folder: HashMap<String, HashSet<String>>,
}

impl FileNameChooser {
    /// Count the identified names over the records the extracted modules
    /// resolve to, one per module (a record reached twice counts twice).
    pub fn new<'r>(records: impl Iterator<Item = &'r FactoryRecord>) -> Self {
        let mut name_counts: HashMap<String, usize> = HashMap::new();
        for record in records {
            if let (Some(name), Some(source)) = (&record.name, record.name_source)
                && source != NameSource::Fallback
            {
                *name_counts.entry(name.clone()).or_insert(0) += 1;
            }
        }
        FileNameChooser {
            name_counts,
            used_by_folder: HashMap::new(),
        }
    }

    /// `chooseFileName`: the on-disk name (without `vendor/` and `.js`) for
    /// one extracted module. Without a named record (the regex-path floor)
    /// the raw factory var is minified residue more often than not, so the
    /// shared filename floor hashes the body (never vendor/H.js).
    pub fn choose(
        &mut self,
        factory_var: &str,
        record: Option<&FactoryRecord>,
        body_text: &str,
    ) -> NameLookup {
        if let Some(record) = record
            && let (Some(name), Some(source)) = (&record.name, record.name_source)
            && !name.is_empty()
        {
            let base = strip_js_extension(name);
            let grouped = !is_hash_fallback_name(name)
                && self.name_counts.get(name.as_str()).is_some_and(|n| *n >= 2);
            let folder = if grouped {
                sanitize_fs_path(&base)
            } else {
                String::new()
            };
            let stem = if grouped {
                stable_stem(record)
            } else {
                sanitize_fs_name(&base)
            };
            let used = self.used_by_folder.entry(folder.clone()).or_default();
            let unique = unique_case_insensitive_name(&stem, used);
            return NameLookup {
                file_name: if folder.is_empty() {
                    unique
                } else {
                    format!("{folder}/{unique}")
                },
                name: name.clone(),
                name_source: source,
                structural_hash: record.structural_hash.clone(),
            };
        }
        let used = self.used_by_folder.entry(String::new()).or_default();
        NameLookup {
            file_name: unique_case_insensitive_name(&vendor_stem_for(factory_var, body_text), used),
            name: factory_var.to_string(),
            name_source: NameSource::Fallback,
            structural_hash: String::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// The manifest (adapters/bun.ts BunModulesManifest + manifest-order.ts)
// ---------------------------------------------------------------------------

/// One written manifest entry (`BunModulesManifestEntry`). The serialized
/// field order matches the TS object literal byte-for-byte; `None` fields
/// are omitted (JSON.stringify drops undefined).
#[derive(Serialize, Clone, Debug)]
pub struct ManifestEntry {
    /// Path of the extracted factory file, relative to the output root
    /// (`vendor/<name>.js`).
    #[serde(rename = "fileName")]
    pub file_name: String,
    /// Human-friendly name used to derive fileName.
    pub name: String,
    /// How `name` was chosen by the cascade.
    #[serde(rename = "nameSource")]
    pub name_source: &'static str,
    /// Structural hash — stable across builds. The cross-version join key.
    /// (The bundle's obfuscated factory variable is DELIBERATELY absent:
    /// Bun rerolls the token every build — exp046 measured the persisted
    /// field at 12,665 lines, 35% of all vendor churn, for no consumer.)
    #[serde(rename = "structuralHash")]
    pub structural_hash: String,
    /// Content-derived identifier every reference to this factory was
    /// rewritten to. Absent when the rewrite was skipped (no resolvable
    /// binding, a WRITE to the factory var, or no capture-free identifier).
    #[serde(rename = "runtimeIdentifier", skip_serializing_if = "Option::is_none")]
    pub runtime_identifier: Option<String>,
    /// Banner package, if a bang-block comment identified the library.
    #[serde(rename = "bannerPackage", skip_serializing_if = "Option::is_none")]
    pub banner_package: Option<String>,
    /// Banner version, if present.
    #[serde(rename = "bannerVersion", skip_serializing_if = "Option::is_none")]
    pub banner_version: Option<String>,
    /// This entry's position within its structuralHash group, in BUNDLE
    /// order — the tie-break `priorNameFor` indexes with. Absent for a
    /// singleton group (the ordinal is always 0 there) and from every
    /// manifest written before exp047. LAST: the TS stamps it with
    /// `{ ...e, hashOrdinal: n }`, which appends the key after every key the
    /// entry literal declared (a present-but-undefined key keeps its slot).
    #[serde(rename = "hashOrdinal", skip_serializing_if = "Option::is_none")]
    pub hash_ordinal: Option<usize>,
}

/// The manifest (`BunModulesManifest`).
#[derive(Serialize, Clone, Debug)]
pub struct BunModulesManifest {
    /// Always "bun" — distinguishes from other adapters that might write
    /// JSON here.
    pub adapter: &'static str,
    /// Filename for the leftover runtime code, if any.
    #[serde(rename = "runtimeFile", skip_serializing_if = "Option::is_none")]
    pub runtime_file: Option<String>,
    /// One entry per extracted CJS factory file.
    pub factories: Vec<ManifestEntry>,
}

impl BunModulesManifest {
    /// The written file's bytes: `JSON.stringify(manifest, null, 2)` + "\n"
    /// (two-space indent, `"key": value`, no trailing spaces — serde_json's
    /// pretty printer writes the same bytes for this all-string/integer
    /// shape).
    pub fn to_written_json(&self) -> String {
        let json = serde_json::to_string_pretty(self).expect("a manifest serializes");
        format!("{json}\n")
    }
}

/// Stamp each entry whose structuralHash is shared with another entry with
/// its ordinal within that group, counted in the order given — which at the
/// call site is BUNDLE order, the order the naming tie-break is defined
/// against (`annotateHashOrdinals`). Singletons are left alone: their
/// ordinal is always 0, so writing it would add a line per entry for no
/// recoverable information.
pub fn annotate_hash_ordinals(mut entries: Vec<ManifestEntry>) -> Vec<ManifestEntry> {
    let mut group_size: HashMap<String, usize> = HashMap::new();
    for e in &entries {
        *group_size.entry(e.structural_hash.clone()).or_insert(0) += 1;
    }
    let mut seen: HashMap<&str, usize> = HashMap::new();
    for e in &mut entries {
        let n = seen.entry(e.structural_hash.as_str()).or_insert(0);
        let ordinal = *n;
        *n += 1;
        if group_size
            .get(e.structural_hash.as_str())
            .copied()
            .unwrap_or(1)
            >= 2
        {
            e.hash_ordinal = Some(ordinal);
        }
    }
    entries
}

/// One prior manifest entry, as the ordering pass reads it (`loadPriorManifestFactories`).
#[derive(Clone, Debug)]
pub struct PriorManifestEntry {
    pub name: String,
    pub structural_hash: String,
}

/// Reorder `fresh` (in bundle order) to follow `prior`'s emitted order
/// (`orderByPriorManifest`). Correspondence in three passes, each consuming
/// prior slots one-to-one: 1. structuralHash (an unchanged library), 2.
/// name (the content changed but the carried-over name held), 3. positional
/// pairing of the leftovers — whatever is still unmatched on each side is by
/// construction the set of entries that changed this release, and pairing
/// the two leftover lists in order returns a changed entry to the slot its
/// own prior version held. An entry that matches nothing at all trails the
/// last anchored entry that preceded it in bundle order (appending them at
/// the end was measured at +494 lines on one hop: relocating an entry is
/// never cheaper than leaving it beside the entries it shipped with).
///
/// Ordering only — no entry's fields are touched.
pub fn order_by_prior_manifest(
    fresh: Vec<ManifestEntry>,
    prior: Option<&[PriorManifestEntry]>,
) -> Vec<ManifestEntry> {
    let Some(prior) = prior else {
        return fresh;
    };
    if prior.is_empty() {
        return fresh;
    }

    let mut claimed: HashSet<usize> = HashSet::new();
    let pool_by = |key: fn(&PriorManifestEntry) -> &str| -> HashMap<String, Vec<usize>> {
        let mut m: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, e) in prior.iter().enumerate() {
            m.entry(key(e).to_string()).or_default().push(i);
        }
        m
    };
    let mut by_hash = pool_by(|e| e.structural_hash.as_str());
    let mut by_name = pool_by(|e| e.name.as_str());

    // Passes 1 and 2, in bundle order so that ties resolve deterministically.
    let mut anchors: Vec<Option<usize>> = Vec::with_capacity(fresh.len());
    for e in &fresh {
        let hash_hit = claim_from(&mut claimed, &mut by_hash, &e.structural_hash);
        let name_hit = if hash_hit.is_some() {
            None
        } else {
            claim_from(&mut claimed, &mut by_name, &e.name)
        };
        anchors.push(hash_hit.or(name_hit));
    }

    // Pass 3: pair the leftovers.
    let leftover: Vec<usize> = (0..prior.len()).filter(|i| !claimed.contains(i)).collect();

    // Position is an (anchor, after) pair. An entry with a prior slot sits
    // AT it. An entry with no slot at all — a genuinely new library on a
    // release that added more entries than it removed — trails the last
    // anchored entry that preceded it in bundle order, so it stays next to
    // its neighbours instead of collecting at the end of the file.
    let mut slots: Vec<(i64, i64)> = Vec::with_capacity(fresh.len());
    let mut next = 0usize;
    let mut last_anchor: i64 = -1;
    let mut after: i64 = 0;
    for (bundle_index, anchor) in anchors.into_iter().enumerate() {
        let mut anchor = anchor;
        if anchor.is_none()
            && let Some(&slot) = leftover.get(next)
        {
            next += 1;
            anchor = Some(slot);
        }
        match anchor {
            Some(a) => {
                last_anchor = a as i64;
                after = 0;
                slots.push((a as i64, 0));
            }
            None => {
                after += 1;
                slots.push((last_anchor, after));
            }
        }
        let _ = bundle_index;
    }

    let mut placed: Vec<(i64, i64, usize, ManifestEntry)> = Vec::with_capacity(fresh.len());
    for (idx, entry) in fresh.into_iter().enumerate() {
        let (anchor, after) = slots[idx];
        placed.push((anchor, after, idx, entry));
    }
    placed.sort_by_key(|&(anchor, after, idx, _)| (anchor, after, idx));
    placed.into_iter().map(|(_, _, _, e)| e).collect()
}

/// Consume the first UNCLAIMED slot for `k` from `pool`
/// (`orderByPriorManifest`'s claimFrom).
fn claim_from(
    claimed: &mut HashSet<usize>,
    pool: &mut HashMap<String, Vec<usize>>,
    k: &str,
) -> Option<usize> {
    let list = pool.get_mut(k)?;
    while !list.is_empty() {
        let c = list.remove(0);
        if !claimed.contains(&c) {
            claimed.insert(c);
            return Some(c);
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Prior-manifest readers (adapters/bun.ts loadPriorVendorNames /
// loadPriorManifestFactories). The TS resolves the manifest FILE from the
// prior tree root (`findPriorTreeRoot`); the CLI here takes the manifest
// path directly — the caller owns the tree-root resolution.
// ---------------------------------------------------------------------------

/// Cross-release vendor names to carry over, read from the prior tree's
/// manifest: structuralHash → the names its factories carried, IN BUNDLE
/// ORDER within the group.
///
/// A LIST per hash, not one name: re-export shims are structurally
/// identical but proxy different libraries, so one hash covers several
/// distinct names. The tie-break is a factory's position within its hash
/// group in BUNDLE order — but the manifest is written in the PRIOR
/// release's order (exp047), so array position no longer supplies it and
/// each group is re-sorted by the `hashOrdinal` field. A manifest written
/// before exp047 has no such field, and there array order IS bundle order,
/// so the fallback reproduces the old behaviour exactly: entries without
/// the field keep their array order and sort after those with one.
pub fn load_prior_vendor_names(manifest_text: &str) -> Option<HashMap<String, Vec<String>>> {
    let manifest: serde_json::Value = serde_json::from_str(manifest_text).ok()?;
    let factories = manifest.get("factories")?.as_array()?;
    let mut groups: BTreeMap<String, Vec<(String, usize, usize)>> = BTreeMap::new();
    for (idx, entry) in factories.iter().enumerate() {
        // `if (!entry.structuralHash || !entry.name) continue;` — a missing,
        // non-string or empty field skips the ENTRY, not the manifest.
        let (Some(hash), Some(name)) = (
            entry.get("structuralHash").and_then(|v| v.as_str()),
            entry.get("name").and_then(|v| v.as_str()),
        ) else {
            continue;
        };
        if hash.is_empty() || name.is_empty() {
            continue;
        }
        let ordinal = entry
            .get("hashOrdinal")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .unwrap_or(usize::MAX);
        groups
            .entry(hash.to_string())
            .or_default()
            .push((name.to_string(), ordinal, idx));
    }
    if groups.is_empty() {
        return None;
    }
    let mut names: HashMap<String, Vec<String>> = HashMap::new();
    for (hash, group) in groups {
        let mut sorted = group;
        sorted.sort_by_key(|(_, ordinal, idx)| (*ordinal, *idx));
        names.insert(hash, sorted.into_iter().map(|(name, _, _)| name).collect());
    }
    Some(names)
}

/// The prior release's manifest entries in the order that release emitted
/// them, for `order_by_prior_manifest`. None when the manifest does not
/// parse or carries no factories — callers then emit in bundle order.
pub fn load_prior_manifest_factories(manifest_text: &str) -> Option<Vec<PriorManifestEntry>> {
    let manifest: serde_json::Value = serde_json::from_str(manifest_text).ok()?;
    let factories = manifest.get("factories")?.as_array()?;
    let entries: Vec<PriorManifestEntry> = factories
        .iter()
        .filter_map(|entry| {
            let name = entry.get("name")?.as_str()?.to_string();
            let structural_hash = entry.get("structuralHash")?.as_str()?.to_string();
            Some(PriorManifestEntry {
                name,
                structural_hash,
            })
        })
        .collect();
    if entries.is_empty() {
        None
    } else {
        Some(entries)
    }
}

// ---------------------------------------------------------------------------
// The LLM fallback pass (vendor-namer.ts)
// ---------------------------------------------------------------------------

/// One batch naming request (`VendorNameRequest`).
#[derive(Clone)]
pub struct VendorNameRequest {
    /// The record's current fallback name (lib_<hash>) — the batch key.
    pub key: String,
    /// Code-derived evidence: export names, URLs, distinctive strings.
    pub evidence: String,
}

/// Batch namer: one proposal or None per request, in request order. The
/// pluggable LLM boundary — the core pass never touches a network.
pub trait VendorNamer {
    fn name_batch(&mut self, requests: Vec<VendorNameRequest>) -> Vec<Option<String>>;
}

/// The namer's system prompt (vendor-namer.ts SYSTEM_PROMPT), byte-exact —
/// it is part of the LLM cache key.
pub const VENDOR_NAMER_SYSTEM_PROMPT: &str = "You identify vendored third-party npm packages inside a decompiled \
JavaScript bundle. For each entry, infer the package's npm name from its \
code evidence (export names, URLs, distinctive string literals). When the \
exact package is unclear, give a short descriptive kebab-case module name \
instead. Never invent a scope. Reply with one name per key.";

/// Names too generic to identify anything (GENERIC_VENDOR_NAMES).
const GENERIC_VENDOR_NAMES: [&str; 17] = [
    "lib", "library", "libs", "module", "modules", "package", "pkg", "vendor", "unknown", "utils",
    "util", "helpers", "helper", "index", "misc", "common", "core",
];

/// JS `\s` — the ASCII subset the evidence regexes' `\s` class uses.
fn is_js_ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\x0b' | b'\x0c' | b'\r')
}

/// JS `\w` minus digits is checked inline; this is the identifier-char test
/// shared by the evidence emulations ([A-Za-z0-9_$]).
fn is_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
}

/// Compact per-factory evidence block for the naming prompt
/// (`buildVendorEvidence`), byte-exact — it IS the prompt the cache keys.
///
/// Emulated regexes over the factory body:
///   URLs     /https?:\/\/[^\s"'`)]+/g          — first 3 unique
///   exports  /\bexports\.([A-Za-z_$][\w$]*)\s*=/g — first 10 unique
///   strings  /["']([^'"\\\n]{4,60})["']/g      — first 10 unique, then
///                                                 filtered to /[a-z]/i
/// Lines: exports, urls, strings, `size: N bytes` (N = UTF-16 length — the
/// TS `.length`); the whole block truncated to `cap_chars` UTF-16 units.
pub fn build_vendor_evidence(body: &str, cap_chars: usize) -> String {
    let urls = evidence_urls(body);
    let exports_props = evidence_exports(body);
    let strings = evidence_strings(body);

    let mut lines: Vec<String> = Vec::new();
    if !exports_props.is_empty() {
        lines.push(format!("exports: {}", exports_props.join(", ")));
    }
    if !urls.is_empty() {
        lines.push(format!("urls: {}", urls.join(" ")));
    }
    if !strings.is_empty() {
        let quoted: Vec<String> = strings.iter().map(|s| json_escape(s)).collect();
        lines.push(format!("strings: {}", quoted.join(", ")));
    }
    // The TS `${body.length}` is UTF-16 code units.
    let size = body.encode_utf16().count();
    lines.push(format!("size: {size} bytes"));
    truncate_utf16(&lines.join("\n"), cap_chars)
}

/// /https?:\/\/[^\s"'`)]+/g — first 3 unique, FULL match (scheme included).
fn evidence_urls(body: &str) -> Vec<String> {
    let bytes = body.as_bytes();
    let mut urls: Vec<String> = Vec::new();
    let mut i = 0usize;
    while i + 4 <= bytes.len() {
        if &bytes[i..i + 4] != b"http" {
            i += 1;
            continue;
        }
        let mut p = i + 4;
        if p < bytes.len() && bytes[p] == b's' {
            p += 1;
        }
        if p >= bytes.len() || bytes[p] != b':' {
            i += 1;
            continue;
        }
        p += 1;
        if !bytes[p..].starts_with(b"//") {
            i += 1;
            continue;
        }
        p += 2;
        let start = p;
        while p < bytes.len()
            && !is_js_ws(bytes[p])
            && !matches!(bytes[p], b'"' | b'\'' | b'`' | b')')
        {
            p += 1;
        }
        if p > start {
            // The TS match INCLUDES the scheme — the full match is the URL.
            let url = body[i..p].to_string();
            if !urls.contains(&url) {
                urls.push(url);
            }
        }
        // matchAll resumes at the end of the match (the capture is greedy,
        // so the next candidate URL cannot start inside it).
        i = p.max(i + 1);
    }
    urls.truncate(3);
    urls
}

/// /\bexports\.([A-Za-z_$][\w$]*)\s*=/g — first 10 unique property names.
fn evidence_exports(body: &str) -> Vec<String> {
    let bytes = body.as_bytes();
    let mut exports_props: Vec<String> = Vec::new();
    let mut i = 0usize;
    while i + 8 <= bytes.len() {
        // The literal `exports.` — the `\b` before is checked below.
        if !bytes[i..].starts_with(b"exports.") {
            i += 1;
            continue;
        }
        // \b: the char before "exports" must be a non-word char.
        let boundary = i == 0 || !is_ident_char(bytes[i - 1]);
        let mut p = i + 8;
        // ([A-Za-z_$][\w$]*) — at least one identifier char.
        let name_start = p;
        if p < bytes.len()
            && (bytes[p].is_ascii_alphabetic() || bytes[p] == b'_' || bytes[p] == b'$')
        {
            p += 1;
            while p < bytes.len() && is_ident_char(bytes[p]) {
                p += 1;
            }
        }
        if boundary && p > name_start {
            // \s* then `=` — the match needs a `=` after zero-or-more
            // whitespace immediately following the capture.
            let mut q = p;
            while q < bytes.len() && is_js_ws(bytes[q]) {
                q += 1;
            }
            if q < bytes.len() && bytes[q] == b'=' {
                let prop = body[name_start..p].to_string();
                if !exports_props.contains(&prop) {
                    exports_props.push(prop);
                    if exports_props.len() > 10 {
                        break;
                    }
                }
                // matchAll resumes AFTER the matched `=`.
                i = q + 1;
                continue;
            }
        }
        i += 1;
    }
    exports_props.truncate(10);
    exports_props
}

/// /["']([^"'\\\n]{4,60})["']/g — greedy {4,60} with backtracking: the
/// LONGEST L in [4, min(run, 60)] whose char at start+L is a quote. First
/// 10 unique that pass /[a-z]/i.
fn evidence_strings(body: &str) -> Vec<String> {
    let bytes = body.as_bytes();
    let mut strings: Vec<String> = Vec::new();
    let mut i = 0usize;
    while i < bytes.len() {
        if !matches!(bytes[i], b'"' | b'\'') {
            i += 1;
            continue;
        }
        let mut run = 0usize;
        while i + 1 + run < bytes.len()
            && run < 60
            && !matches!(bytes[i + 1 + run], b'"' | b'\'' | b'\\' | b'\n')
        {
            run += 1;
        }
        let mut matched: Option<usize> = None;
        let mut l = run.min(60);
        while l >= 4 {
            let close = i + 1 + l;
            if close < bytes.len() && matches!(bytes[close], b'"' | b'\'') {
                matched = Some(l);
                break;
            }
            l -= 1;
        }
        if let Some(l) = matched {
            let s = body[i + 1..i + 1 + l].to_string();
            // /[a-z]/i filter, unique (first occurrence wins).
            if s.bytes().any(|b| b.is_ascii_alphabetic()) && !strings.contains(&s) {
                strings.push(s);
                if strings.len() > 10 {
                    break;
                }
            }
            i = i + 1 + l + 1;
        } else {
            i += 1;
        }
    }
    strings.truncate(10);
    strings
}

/// JSON.stringify of one string, for the evidence line (serde_json's
/// string serializer matches the JS escaping for valid UTF-8 input).
fn json_escape(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| format!("\"{s}\""))
}

/// Truncate to at most `cap` UTF-16 code units (the TS `.slice(0, cap)`).
fn truncate_utf16(s: &str, cap: usize) -> String {
    if s.encode_utf16().count() <= cap {
        return s.to_string();
    }
    let mut out = String::new();
    let mut units = 0usize;
    for c in s.chars() {
        let len = c.len_utf16();
        if units + len > cap {
            break;
        }
        units += len;
        out.push(c);
    }
    out
}

/// Validate one proposal into a package-shaped vendor name (lowercased,
/// optional @scope/, dots/dashes allowed), or None when it is generic,
/// minified-short, or malformed (`acceptVendorName`).
///
/// Emulated: `/^(@[a-z0-9][a-z0-9._-]*\/)?[a-z0-9][a-z0-9._-]{2,39}$/`.
pub fn accept_vendor_name(proposal: &str) -> Option<String> {
    let name = proposal.trim().to_lowercase();
    let b = name.as_bytes();
    let mut p = 0usize;
    // The optional @scope/ group.
    if b.first() == Some(&b'@') {
        p += 1;
        let head = p;
        while p < b.len() && (b[p].is_ascii_alphanumeric()) {
            p += 1;
        }
        if p == head {
            return None; // at least one [a-z0-9] after the @
        }
        // [a-z0-9._-]* then a literal `/`.
        while p < b.len() && (b[p].is_ascii_alphanumeric() || matches!(b[p], b'.' | b'_' | b'-')) {
            p += 1;
        }
        if p >= b.len() || b[p] != b'/' {
            return None;
        }
        p += 1;
    }
    // [a-z0-9][a-z0-9._-]{2,39}$ — 3..=40 chars of that class.
    let body_start = p;
    if p >= b.len() || !b[p].is_ascii_alphanumeric() {
        return None;
    }
    p += 1;
    while p < b.len() && (b[p].is_ascii_alphanumeric() || matches!(b[p], b'.' | b'_' | b'-')) {
        p += 1;
    }
    let body_len = p - body_start;
    if !(3..=40).contains(&body_len) {
        return None;
    }
    if p != b.len() {
        return None;
    }
    if GENERIC_VENDOR_NAMES.contains(&name.as_str()) {
        return None;
    }
    Some(name)
}

/// Prompt for one batch of vendor-name requests (`buildPrompt`), byte-exact
/// — it is the cache key's `code`/`userPrompt`.
pub fn build_prompt(requests: &[VendorNameRequest]) -> String {
    let mut lines: Vec<String> = Vec::new();
    lines.push(format!(
        "Identify {} vendored modules extracted from a JavaScript bundle.",
        requests.len()
    ));
    lines.push(String::new());
    for request in requests {
        lines.push(format!("### {}", request.key));
        lines.push(request.evidence.clone());
        lines.push(String::new());
    }
    let reply_keys = requests
        .iter()
        .map(|r| format!("\"{}\": \"<npm package or kebab-case name>\"", r.key))
        .collect::<Vec<_>>()
        .join(", ");
    lines.push(format!("Reply with JSON {{{reply_keys}}}."));
    lines.join("\n")
}

/// Re-name every `fallback`-named factory record through the namer,
/// chunked (`nameFallbackFactoriesWithLlm`). Accepted proposals become the
/// record's name with nameSource "llm"; everything else keeps the
/// lib_<hash> fallback. A name the model applies to more than
/// `max_shared_name` distinct modules is a hallucinated default (the real
/// run gave 100 modules "is-plain-object") — those are ALL reverted to
/// their lib_<hash> fallback, honest over confidently-wrong. Returns how
/// many records ended up llm-named. Mutates the records, mirroring
/// `nameCjsFactories`.
///
/// The TS fans the chunks out with Promise.all and collects into a shared
/// `named` list whose ORDER depends on resolution order — but the order is
/// provably inert (the over-application census is a count per name, and the
/// final assignment is per-record), so this port walks the chunks
/// sequentially and gets the same records named.
pub fn name_fallback_factories_with_llm(
    factories: &mut [FactoryRecord],
    source: &str,
    namer: &mut dyn VendorNamer,
) -> usize {
    name_fallback_factories_with_llm_sized(factories, source, namer, 24, 40)
}

/// The same pass with the chunk/over-application constants exposed (the
/// TS's default arguments; the tests exercise them).
pub fn name_fallback_factories_with_llm_sized(
    factories: &mut [FactoryRecord],
    source: &str,
    namer: &mut dyn VendorNamer,
    chunk_size: usize,
    max_shared_name: usize,
) -> usize {
    let fallbacks: Vec<usize> = factories
        .iter()
        .enumerate()
        .filter(|(_, f)| f.name_source == Some(NameSource::Fallback) && f.name.is_some())
        .map(|(i, _)| i)
        .collect();
    let mut named: Vec<(usize, String)> = Vec::new();
    for chunk in fallbacks.chunks(chunk_size) {
        let requests: Vec<VendorNameRequest> = chunk
            .iter()
            .map(|&idx| {
                let record = &factories[idx];
                VendorNameRequest {
                    key: record.name.clone().unwrap_or_default(),
                    evidence: build_vendor_evidence(
                        &source[record.span.start as usize..record.span.end as usize],
                        700,
                    ),
                }
            })
            .collect();
        let proposals = namer.name_batch(requests);
        for (pos, &idx) in chunk.iter().enumerate() {
            let accepted = proposals
                .get(pos)
                .and_then(|p| p.as_deref())
                .and_then(accept_vendor_name);
            if let Some(name) = accepted {
                named.push((idx, name));
            }
        }
    }

    let mut counts: HashMap<&str, usize> = HashMap::new();
    for (_, name) in &named {
        *counts.entry(name.as_str()).or_insert(0) += 1;
    }
    let over_applied: HashSet<String> = counts
        .iter()
        .filter(|(_, n)| **n > max_shared_name)
        .map(|(name, _)| (*name).to_string())
        .collect();

    let mut renamed = 0;
    for (idx, name) in named {
        if over_applied.contains(&name) {
            continue; // keep the honest lib_<hash>
        }
        let record = &mut factories[idx];
        record.name = Some(name);
        record.name_source = Some(NameSource::Llm);
        renamed += 1;
    }
    renamed
}

// ---------------------------------------------------------------------------
// The provider-backed namer (the CLI's LLM boundary)
// ---------------------------------------------------------------------------

/// The batch request the TS vendor namer sends (`createVendorNamer`): the
/// built prompt as BOTH `code` and `userPrompt`, the batch keys as
/// `identifiers`, an empty used-name set, no callees or callsites, the
/// namer's system prompt. Its cache key is `humanify_model::llm::
/// cache_key_of` — the one owner of the key — so a vendor batch replays the
/// entry the TS run wrote.
pub fn vendor_batch_request(requests: &[VendorNameRequest]) -> BatchRenameRequest {
    let prompt = build_prompt(requests);
    BatchRenameRequest {
        code: prompt.clone(),
        identifiers: requests.iter().map(|r| r.key.clone()).collect(),
        system_prompt: Some(VENDOR_NAMER_SYSTEM_PROMPT.to_string()),
        user_prompt: Some(prompt),
        ..BatchRenameRequest::default()
    }
}

/// A `VendorNamer` over any `NameProvider` (the TS `createVendorNamer`
/// over an `LLMProvider`): one call per batch, the proposal for each key
/// read off the response's renames. A failed batch (a provider error — for
/// a replay-only provider, a cache MISS) answers all-null and is counted:
/// a miss means the oracle run never asked this prompt (our prompt bytes
/// diverged — a parity break) or its response was all-null and hence never
/// cached (the TS writes only non-empty responses).
/// Why vendor names were or were not produced, for one run
/// (`VendorNamingStats`): three null outcomes were indistinguishable on disk
/// until they were counted apart.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct VendorNamingStats {
    /// The model proposed a usable name.
    pub named: usize,
    /// The model returned nothing for that key.
    pub declined: usize,
    /// The model echoed the key back — a non-answer, not a name.
    pub echoed: usize,
    /// Whole batches lost to a provider error. Not declines.
    pub batches_failed: usize,
}

/// `classifyProposal`: the proposal for one key, counted. Nothing (absent,
/// null or the empty string — JS `!proposed`) is a decline; the key echoed
/// back is a non-answer; anything else is a name (validated later by
/// `accept_vendor_name`).
fn classify_proposal(
    proposed: Option<&str>,
    key: &str,
    stats: &mut VendorNamingStats,
) -> Option<String> {
    match proposed {
        None | Some("") => {
            stats.declined += 1;
            None
        }
        Some(p) if p == key => {
            stats.echoed += 1;
            None
        }
        Some(p) => {
            stats.named += 1;
            Some(p.to_string())
        }
    }
}

pub struct ProviderVendorNamer<'p> {
    provider: &'p dyn NameProvider,
    /// The per-outcome tally.
    pub stats: VendorNamingStats,
}

impl<'p> ProviderVendorNamer<'p> {
    pub fn new(provider: &'p dyn NameProvider) -> Self {
        ProviderVendorNamer {
            provider,
            stats: VendorNamingStats::default(),
        }
    }
}

impl VendorNamer for ProviderVendorNamer<'_> {
    fn name_batch(&mut self, requests: Vec<VendorNameRequest>) -> Vec<Option<String>> {
        if requests.is_empty() {
            return Vec::new();
        }
        let request = vendor_batch_request(&requests);
        let call = LlmCall {
            system_prompt: VENDOR_NAMER_SYSTEM_PROMPT.to_string(),
            user_prompt: request.code.clone(),
            request,
        };
        match self.provider.run_wave(vec![call]).pop() {
            Some(Ok(response)) => requests
                .iter()
                .map(|r| classify_proposal(response.renames.get(&r.key), &r.key, &mut self.stats))
                .collect(),
            _ => {
                self.stats.batches_failed += 1;
                requests.iter().map(|_| None).collect()
            }
        }
    }
}
