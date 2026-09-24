//! Bun CJS module classification (WP1.5) — TS originals:
//! `src/analysis/bun-module-classification.ts`, plus the one function WP1.5
//! needs from `src/shared/bun-helpers.ts` (identifyBunCjsFactory — that
//! file's ledger row is WP5.3 → core::emit; the rest of it stays there).
//! Submodules: `wrapper` (wrapper-detection.ts), `soundness` (soundness.ts),
//! `known_globals` (known-globals.ts).
//!
//! Bun wraps every CJS module in a `var X = HELPER((q,m) => {...})` factory
//! that survives minification; factories are essentially guaranteed
//! third-party, so the rename pipeline skips their bindings/functions and
//! the split extracts them to vendor/. Classifying them is what makes the
//! function graph's member set match the TS oracle's (the factory-body
//! functions are skipped there).
//!
//! Scope containment by SPAN, not by scope object: a factory body's span
//! contains exactly the nodes in its scope (well-nested code), and spans
//! are the dump's identity — no scope-map plumbing. The TS's scope-walk
//! (isInsideFactoryBody's 64-deep parent chain) and a span containment
//! test agree on well-nested code; the four-pair gate is the judge.
//!
//! Every regex from the TS is emulated byte-for-byte WITHOUT a regex
//! crate — each emulation documents the exact pattern it mirrors, and the
//! probe (test/parity/wp1.5-probe.mjs) holds the TS-derived expectations.

use std::collections::HashMap;

use oxc_ast::AstKind;
use oxc_estree::{CompactSerializer, ESTree};
use oxc_span::{GetSpan, Span};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::babel_view::unparen;
use crate::hash::serialize::{LiteralPolicy, SymbolTables, canonical_serialize};

pub mod known_globals;
pub mod soundness;
pub mod vendor_dump;
pub mod vendor_names;
pub mod wrapper;

/// The identified CJS factory helper (bun-helpers.ts's IdentifiedHelper).
pub struct IdentifiedHelper {
    pub name: String,
    pub start_offset: u32,
}

/// JS `\s` — the ASCII subset a minified bundle's markers use. (The TS
/// regex `\s` also takes Unicode spaces; a non-ASCII whitespace byte inside
/// `{exports: … {}}` or around an `=` would be a parity miss — the gate is
/// the judge on the real bundles.)
fn is_js_ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\x0b' | b'\x0c' | b'\r')
}

fn starts_with_at(bytes: &[u8], at: usize, pat: &[u8]) -> bool {
    bytes.len() >= at + pat.len() && &bytes[at..at + pat.len()] == pat
}

/// The marker Bun emits inside its CJS factory helper's body — the TS
/// pattern is `/\{exports:\s*\{\}\}/g`: the whitespace after the colon is
/// OPTIONAL (the real minified bundles carry `{exports:{}}`, no space).
const MARKER_HEAD: &[u8] = b"{exports:";
const MARKER_TAIL: &[u8] = b"{}}";

/// Scan the source for the factory-body marker and recover the binding that
/// declares the helper (bun-helpers.ts:30-56): the LEFTMOST match of
/// `(?:(?:var|let|const)\s+|,)([$\w]+)\s*=\s*[^;]*$` over the bounded
/// lookback window (2000 chars — a bounded window keeps a miss from
/// re-slicing an ever-growing prefix of a 20MB source). `.match()` without
/// /g takes the leftmost match; on a comma-joined declaration that is the
/// FIRST declarator, not the one nearest the marker.
pub fn identify_bun_cjs_factory(source: &str) -> Option<IdentifiedHelper> {
    let bytes = source.as_bytes();
    let mut search_from = 0usize;
    loop {
        let head = find_sub(bytes, search_from, MARKER_HEAD)?;
        // `\s*` between the head and `{}}`; a miss continues the scan one
        // byte past the head (the TS regex engine advances char-wise on a
        // failed attempt), a hit past the whole match.
        let mut p = head + MARKER_HEAD.len();
        while p < bytes.len() && is_js_ws(bytes[p]) {
            p += 1;
        }
        if !starts_with_at(bytes, p, MARKER_TAIL) {
            search_from = head + 1;
            continue;
        }
        let slice_start = lookback_start(source, head, 2000);
        let before = &source[slice_start..head];
        if let Some((match_start, name)) = leftmost_binding_match(before) {
            return Some(IdentifiedHelper {
                name,
                start_offset: (slice_start + match_start) as u32,
            });
        }
        search_from = p + MARKER_TAIL.len();
    }
}

/// The byte offset of JS `Math.max(0, at - units)` for byte offset `at`:
/// `units` UTF-16 code units back, never inside a char. Where the JS index
/// would land between a surrogate pair's halves, the JS slice starts with a
/// lone low surrogate — a character no pattern here can match — so starting
/// after the whole char is equivalent. (Counting bytes instead shrank the
/// window on non-ASCII text and could slice mid-char — a panic.)
fn lookback_start(source: &str, at: usize, units: usize) -> usize {
    let mut used = 0;
    for (i, c) in source[..at].char_indices().rev() {
        used += c.len_utf16();
        if used > units {
            return i + c.len_utf8();
        }
    }
    0
}

fn find_sub(bytes: &[u8], from: usize, pat: &[u8]) -> Option<usize> {
    if from >= bytes.len() {
        return None;
    }
    bytes[from..]
        .windows(pat.len())
        .position(|w| w == pat)
        .map(|r| r + from)
}

/// `(?:(?:var|let|const)\s+|,)([$\w]+)\s*=\s*[^;]*$` — the LEFTMOST match
/// over `before`: position 0..len in order; at each position the keyword
/// alternatives, then the comma; the first position whose remainder matches
/// wins. The regex is unanchored, so a position inside a word can match
/// (`covar x=...` matches at the embedded "var") — the scan is per byte.
fn leftmost_binding_match(before: &str) -> Option<(usize, String)> {
    let bytes = before.as_bytes();
    for i in 0..bytes.len() {
        let keyword_end = if starts_with_at(bytes, i, b"const") {
            Some(i + 5)
        } else if starts_with_at(bytes, i, b"var") || starts_with_at(bytes, i, b"let") {
            Some(i + 3)
        } else {
            None
        };
        if let Some(after_kw) = keyword_end {
            // `\s+` — at least one whitespace char.
            let mut j = after_kw;
            let ws_start = j;
            while j < bytes.len() && is_js_ws(bytes[j]) {
                j += 1;
            }
            if j > ws_start
                && let Some(name) = name_assign_tail(bytes, j)
            {
                return Some((i, name));
            }
        }
        if bytes[i] == b','
            && let Some(name) = name_assign_tail(bytes, i + 1)
        {
            return Some((i, name));
        }
    }
    None
}

/// `([$\w]+)\s*=\s*[^;]*$` from `j`: an identifier-ish name (`\w` plus `$`,
/// ASCII — the TS class), optional whitespace, `=`, optional whitespace,
/// then a tail to the end of the window with NO semicolon.
fn name_assign_tail(bytes: &[u8], mut j: usize) -> Option<String> {
    let name_start = j;
    while j < bytes.len()
        && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_' || bytes[j] == b'$')
    {
        j += 1;
    }
    if j == name_start {
        return None;
    }
    let name = String::from_utf8_lossy(&bytes[name_start..j]).into_owned();
    while j < bytes.len() && is_js_ws(bytes[j]) {
        j += 1;
    }
    if bytes.get(j) != Some(&b'=') {
        return None;
    }
    j += 1;
    while j < bytes.len() && is_js_ws(bytes[j]) {
        j += 1;
    }
    if bytes[j..].contains(&b';') {
        return None;
    }
    Some(name)
}

/// Where a factory's name came from (the TS nameSource union). "llm" is set
/// post-cascade by the adapter's LLM pass (`vendor_names::
/// name_fallback_factories_with_llm`) over fallback records only.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NameSource {
    Banner,
    Url,
    CarryOver,
    Llm,
    Fallback,
}

impl NameSource {
    /// The TS manifest's `nameSource` string — the written field, byte-exact.
    pub fn as_str(self) -> &'static str {
        match self {
            NameSource::Banner => "banner",
            NameSource::Url => "url",
            NameSource::CarryOver => "carry-over",
            NameSource::Llm => "llm",
            NameSource::Fallback => "fallback",
        }
    }
}

/// One detected CJS factory (CjsFactoryRecord's dumped fields).
pub struct FactoryRecord {
    /// The minified handle (the declarator's id name).
    pub factory_var: String,
    /// The whole VariableDeclarator's span (byteRange).
    pub span: Span,
    /// The factory BODY function's span (the scope-containment region).
    pub body_span: Span,
    /// 1-indexed start/end line of the declarator (the TS lineRange).
    pub line_range: (u32, u32),
    /// First 16 hex of sha256 of the DECLARATOR's source slice (the TS
    /// contentHash — in-bundle dedup; NOT cross-version-stable, Bun re-rolls
    /// the minified identifiers).
    pub content_hash: String,
    /// The body's structural hash (blurred) — the cross-version join key.
    pub structural_hash: String,
    /// The banner's stripped, trimmed text — a bang-block's value minus the
    /// leading `!` (the TS parseBanner stores text = raw.replace(/^!/,'').trim(),
    /// so the `!` never reaches the record).
    pub banner_text: Option<String>,
    pub banner_package: Option<String>,
    pub banner_version: Option<String>,
    /// Final assigned name — set by the naming cascade (`nameCjsFactories`).
    pub name: Option<String>,
    /// Where the name came from — set by the naming cascade.
    pub name_source: Option<NameSource>,
}

/// The classification: the helper var + every factory, in source order.
pub struct BunModuleClassification {
    pub helper_var: String,
    pub factories: Vec<FactoryRecord>,
    /// WHERE each vendor name came from, once `name_cjs_factories` has run.
    /// Undefined until then — absent is not "all zero" (the TS note).
    pub name_counts: Option<FactoryNameCounts>,
}

/// Counts of factories named by each source (FactoryNameCounts).
#[derive(Default)]
pub struct FactoryNameCounts {
    pub banner: usize,
    pub url: usize,
    pub carry_over: usize,
    pub llm: usize,
    pub fallback: usize,
}

/// True when a span sits inside ANY factory body (the isInsideFactoryBody
/// skip, by span containment).
pub fn is_inside_factory_body(span: Span, factories: &[FactoryRecord]) -> bool {
    factories
        .iter()
        .any(|f| span.start >= f.body_span.start && span.end <= f.body_span.end)
}

/// `lib_<first 8 chars of structuralHash>` (hashFallbackName).
pub fn hash_fallback_name(structural_hash: &str) -> String {
    format!("lib_{}", &structural_hash[..8.min(structural_hash.len())])
}

/// True when `name` is the hash fallback — identifies NO package, only that
/// nothing else identified the module. Callers deciding whether a name is a
/// real library identity must test THIS rather than the name source: a
/// fallback name carried over from a prior release arrives as carry-over,
/// and a source test silently flips on the second hop.
pub fn is_hash_fallback_name(name: &str) -> bool {
    // /^lib_[0-9a-f]{8}$/
    let Some(rest) = name.strip_prefix("lib_") else {
        return false;
    };
    rest.len() == 8
        && rest
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Per-factory position within its structuralHash group, plus each group's
/// size. One hash can legitimately cover SEVERAL DISTINCT modules (re-export
/// shims are structurally identical but proxy different libraries), so a
/// hash alone cannot key a name; position within the group does.
struct HashIndex {
    /// Position of each factory (by Vec index) within its hash group.
    occurrence: Vec<usize>,
    group_size: HashMap<String, usize>,
}

fn index_by_hash(factories: &[FactoryRecord]) -> HashIndex {
    let mut occurrence = Vec::with_capacity(factories.len());
    let mut group_size: HashMap<String, usize> = HashMap::new();
    for factory in factories {
        let n = group_size
            .entry(factory.structural_hash.clone())
            .or_insert(0);
        occurrence.push(*n);
        *n += 1;
    }
    HashIndex {
        occurrence,
        group_size,
    }
}

/// The prior release's name for this factory, or None. The group must be
/// INTACT — the same number of factories share the hash now as did in the
/// prior — or positions no longer line up and carrying would silently
/// misname every member. A changed group earns a fresh name, never a guess.
fn prior_name_for(
    idx: usize,
    hash: &str,
    prior_names: Option<&HashMap<String, Vec<String>>>,
    index: &HashIndex,
) -> Option<String> {
    let group = prior_names?.get(hash)?;
    if group.len() != *index.group_size.get(hash)? {
        return None;
    }
    group.get(index.occurrence[idx]).cloned()
}

/// Apply the Phase 3 naming cascade to each classified factory.
///
/// Priority order (first hit wins): banner-derived, distinctive URL
/// (github.com/<org>/<repo> or *.dev/.org domains), cross-bundle carry-over
/// via `prior_names`, then the structural-hash fallback. The LLM pass runs
/// POST-cascade in the unpack adapter over fallback-named records only —
/// deterministic sources always win and this cascade stays sync/pure.
///
/// The fallback and carry-over keys are deliberately the STRUCTURAL hash,
/// not the raw content hash: Bun re-rolls minified identifiers between
/// builds, so the content hash would change every release for unchanged
/// libraries and defeat stable filenames.
pub fn name_cjs_factories(
    classification: &mut BunModuleClassification,
    source: &str,
    prior_names: Option<&HashMap<String, Vec<String>>>,
) -> FactoryNameCounts {
    let mut counts = FactoryNameCounts::default();
    let index = index_by_hash(&classification.factories);
    for (idx, factory) in classification.factories.iter_mut().enumerate() {
        if let Some(pkg) = &factory.banner_package {
            let version = factory.banner_version.clone();
            factory.name = Some(match version {
                Some(v) => format!("{pkg}@{v}"),
                None => pkg.clone(),
            });
            factory.name_source = Some(NameSource::Banner);
            counts.banner += 1;
            continue;
        }
        let body_source = &source[factory.span.start as usize..factory.span.end as usize];
        if let Some(url_name) = extract_distinctive_repo_name(body_source) {
            factory.name = Some(url_name);
            factory.name_source = Some(NameSource::Url);
            counts.url += 1;
            continue;
        }
        if let Some(carried) = prior_name_for(idx, &factory.structural_hash, prior_names, &index) {
            factory.name = Some(carried);
            factory.name_source = Some(NameSource::CarryOver);
            counts.carry_over += 1;
            continue;
        }
        factory.name = Some(hash_fallback_name(&factory.structural_hash));
        factory.name_source = Some(NameSource::Fallback);
        counts.fallback += 1;
    }
    counts
}

/// Look for a single distinctive package/repo reference in `body_source`.
///
/// A "distinctive" reference is one that appears with exactly one unique
/// org/repo pair (or one unique pkg subdomain). If the body cites multiple
/// different repos, we abstain — the signal is no longer reliable.
///
/// Emulates, case-insensitively:
///   `github\.com\/([a-z0-9][a-z0-9._-]*)\/([a-z0-9][a-z0-9._-]*)`
///   `\b([a-z0-9][a-z0-9-]*)\.(?:dev|io|org)\b`
pub fn extract_distinctive_repo_name(body_source: &str) -> Option<String> {
    let bytes = body_source.as_bytes();
    // ── github.com/<org>/<repo> ──────────────────────────────────────────
    let mut repos: Vec<String> = Vec::new(); // unique org/repo pairs
    let mut i = 0usize;
    while i < bytes.len() {
        let Some(hit) = find_ci(bytes, i, b"github.com/") else {
            break;
        };
        let mut p = hit + b"github.com/".len();
        let Some((org, np)) = capture_run(bytes, p) else {
            i = hit + 1;
            continue;
        };
        p = np;
        if p >= bytes.len() || bytes[p] != b'/' {
            i = hit + 1;
            continue;
        }
        p += 1;
        let Some((repo, np)) = capture_run(bytes, p) else {
            i = hit + 1;
            continue;
        };
        let pair = format!("{org}/{repo}");
        if !repos.contains(&pair) {
            repos.push(pair);
            if repos.len() > 1 {
                break;
            }
        }
        i = np;
    }
    if repos.len() == 1 {
        let only = &repos[0];
        let slash = only.find('/').expect("org/repo has a slash");
        return Some(only[slash + 1..].to_string());
    }
    if !repos.is_empty() {
        return None; // two or more distinct repos — abstain
    }

    // ── *.dev / *.io / *.org subdomains ─────────────────────────────────
    let mut pkgs: Vec<String> = Vec::new();
    for tld in [&b".dev"[..], &b".io"[..], &b".org"[..]] {
        let mut i = 0usize;
        while let Some(hit) = find_ci(bytes, i, tld) {
            // Walk back over [a-z0-9-] (case-insensitive; `_` is NOT in
            // the class) — the capture.
            let mut start = hit;
            while start > 0
                && (bytes[start - 1].is_ascii_alphanumeric() || bytes[start - 1] == b'-')
            {
                start -= 1;
            }
            // `\b` before: the char before the run must be a non-word char
            // (or the string start). `\w` = alnum + `_`.
            let boundary_before = start == 0
                || !(bytes[start - 1].is_ascii_alphanumeric() || bytes[start - 1] == b'_');
            // `[a-z0-9]` head — a leading `-` fails the first-char class
            // even though the walk-back included it.
            let first_ok = start < hit && bytes[start].is_ascii_alphanumeric();
            // `\b` after: the char after the TLD must be a non-word char
            // (or the end).
            let after = hit + tld.len();
            let boundary_after = after == bytes.len()
                || !(bytes[after].is_ascii_alphanumeric() || bytes[after] == b'_');
            if boundary_before && first_ok && boundary_after {
                let name = String::from_utf8_lossy(&bytes[start..hit]).into_owned();
                if !pkgs.contains(&name) {
                    pkgs.push(name);
                    if pkgs.len() > 1 {
                        return None;
                    }
                }
            }
            i = hit + 1;
        }
    }
    if pkgs.len() == 1 {
        return Some(pkgs.into_iter().next().expect("one unique pkg"));
    }
    None
}

/// Case-insensitive substring search (ASCII only).
fn find_ci(bytes: &[u8], from: usize, pat: &[u8]) -> Option<usize> {
    let mut i = from;
    while i + pat.len() <= bytes.len() {
        if bytes[i..i + pat.len()]
            .iter()
            .zip(pat)
            .all(|(a, b)| a.eq_ignore_ascii_case(b))
        {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// Maximal run of the github capture class `[a-z0-9._-]` (case-insensitive)
/// starting at `at`.
fn capture_run(bytes: &[u8], at: usize) -> Option<(String, usize)> {
    let mut j = at;
    while j < bytes.len()
        && (bytes[j].is_ascii_alphanumeric() || matches!(bytes[j], b'.' | b'_' | b'-'))
    {
        j += 1;
    }
    if j == at {
        return None;
    }
    Some((String::from_utf8_lossy(&bytes[at..j]).into_owned(), j))
}

/// Classify the factories of a parsed bundle. `wrapper_body` = the wrapper
/// function's BODY block span when a wrapper exists (the container of the
/// top-level statements), else None (the program's own body is the
/// container).
pub fn classify_bun_modules<'a>(
    source: &'a str,
    program: &'a oxc_ast::ast::Program<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
    wrapper_body: Option<Span>,
    tables: &SymbolTables,
) -> Option<BunModuleClassification> {
    let helper = identify_bun_cjs_factory(source)?;

    // The container's statements: the wrapper body's (when the body is a
    // block — the TS bodyPath.isBlockStatement() gate) or the program's.
    let nodes = semantic.nodes();
    let container_stmts: Vec<Span> = match wrapper_body {
        Some(body) => nodes
            .iter()
            .find(|n| n.span() == body && function_body_kind(&n.kind()))
            .and_then(|n| match n.kind() {
                // oxc stores a function's body as FunctionBody (Babel: the
                // body IS a BlockStatement) — the wrapper container is the
                // function body's statement list.
                AstKind::FunctionBody(b) => {
                    Some(b.statements.iter().map(|s| s.span()).collect::<Vec<_>>())
                }
                AstKind::BlockStatement(b) => {
                    Some(b.body.iter().map(|s| s.span()).collect::<Vec<_>>())
                }
                _ => None,
            })
            .unwrap_or_default(),
        None => program.body.iter().map(|s| s.span()).collect(),
    };

    let comments = semantic.comments();
    let mut line_starts: Vec<u32> = vec![0];
    for (i, b) in source.bytes().enumerate() {
        if b == b'\n' {
            line_starts.push(i as u32 + 1);
        }
    }

    let mut factories: Vec<FactoryRecord> = Vec::new();
    // ONE pass over the nodes: a linear `nodes.iter().find` per container
    // statement is O(statements x nodes) — quadratic on a 12MB bundle (the
    // wrapper body holds the whole program's statements). The container
    // statement spans go into a set; every VariableDeclaration whose span
    // is in it is a container statement, in node order (= source order).
    use std::collections::HashSet;
    let container_set: HashSet<(u32, u32)> =
        container_stmts.iter().map(|s| (s.start, s.end)).collect();
    let mut stmt_index: HashMap<(u32, u32), usize> = HashMap::new();
    for (i, sp) in container_stmts.iter().enumerate() {
        stmt_index.insert((sp.start, sp.end), i);
    }
    let container_declarations = nodes
        .iter()
        .filter(|n| container_set.contains(&(n.span().start, n.span().end)))
        .filter_map(|n| match n.kind() {
            AstKind::VariableDeclaration(d) => Some((n.span(), d)),
            _ => None,
        })
        .collect::<Vec<_>>();
    for (stmt_span, var_decl) in container_declarations {
        let i = stmt_index[&(stmt_span.start, stmt_span.end)];
        // The banner attached at statement level: the LAST bang-block in the
        // gap between the prior sibling's end (or the container's start for
        // the first statement) and this statement's start.
        let gap_start = if i == 0 {
            wrapper_body.map(|w| w.start).unwrap_or(0)
        } else {
            container_stmts[i - 1].end
        };
        let stmt_banner = collect_banner(source, comments, gap_start, stmt_span.start);
        for decl in var_decl.declarations.iter() {
            let declarator_span = decl.span();
            let Some(init) = decl.init.as_ref() else {
                continue;
            };
            let oxc_ast::ast::Expression::CallExpression(call) = init else {
                continue;
            };
            let oxc_ast::ast::Expression::Identifier(callee) = unparen(&call.callee) else {
                continue;
            };
            if callee.name != helper.name.as_str() {
                continue;
            }
            let Some(arg0) = call.arguments.first() else {
                continue;
            };
            let Some(arg_expr) = arg0.as_expression() else {
                continue;
            };
            let arg_expr = unparen(arg_expr);
            if !matches!(
                arg_expr,
                oxc_ast::ast::Expression::ArrowFunctionExpression(_)
                    | oxc_ast::ast::Expression::FunctionExpression(_)
            ) {
                continue;
            }
            let body_span = arg_expr.span();
            // The banner: statement-level first (even WITHOUT a package —
            // bannerText is recorded either way), the in-body fallback only
            // when statement level found nothing (the TS
            // `banner ?? findBannerInsideBody(arg0)`).
            let banner = stmt_banner
                .clone()
                .or_else(|| find_banner_inside_body(source, comments, body_span));
            // The structural hash of the body function (blurred) — the
            // cross-version join key, via the same canonical serializer the
            // function graph uses.
            let structural_hash = {
                let mut ser = CompactSerializer::new(false, false);
                match arg_expr {
                    oxc_ast::ast::Expression::ArrowFunctionExpression(a) => a.serialize(&mut ser),
                    oxc_ast::ast::Expression::FunctionExpression(f) => f.serialize(&mut ser),
                    _ => unreachable!("shape-checked above"),
                }
                let estree = ser.into_string();
                let mut de = serde_json::Deserializer::from_str(&estree);
                de.disable_recursion_limit();
                let body_json: Value = Deserialize::deserialize(&mut de).unwrap_or(Value::Null);
                canonical_serialize(&body_json, tables, LiteralPolicy::Blurred).hash
            };
            // The content hash covers the DECLARATOR's slice (the TS
            // contentHash), not the body's.
            let content_hash: String = {
                let decl_source =
                    &source[declarator_span.start as usize..declarator_span.end as usize];
                let digest = Sha256::digest(decl_source.as_bytes());
                digest[..8].iter().map(|b| format!("{b:02x}")).collect()
            };
            let factory_var = match &decl.id {
                oxc_ast::ast::BindingPattern::BindingIdentifier(id) => id.name.to_string(),
                _ => "<destructured>".to_string(),
            };
            factories.push(FactoryRecord {
                factory_var,
                span: declarator_span,
                body_span,
                line_range: (
                    line_of(declarator_span.start, &line_starts),
                    line_of(declarator_span.end, &line_starts),
                ),
                content_hash,
                structural_hash,
                banner_text: banner.as_ref().map(|b| b.text.clone()),
                banner_package: banner.as_ref().and_then(|b| b.pkg.clone()),
                banner_version: banner.as_ref().and_then(|b| b.version.clone()),
                name: None,
                name_source: None,
            });
        }
    }

    Some(BunModuleClassification {
        helper_var: helper.name,
        factories,
        name_counts: None,
    })
}

/// The container-body node kinds: a function's body (oxc FunctionBody;
/// Babel BlockStatement) or a bare block.
fn function_body_kind(kind: &AstKind<'_>) -> bool {
    matches!(kind, AstKind::FunctionBody(_) | AstKind::BlockStatement(_))
}

fn line_of(offset: u32, line_starts: &[u32]) -> u32 {
    match line_starts.binary_search(&offset) {
        Ok(i) => i as u32 + 1,
        Err(i) => i as u32,
    }
}

/// The statement's VariableDeclaration (when the statement IS one).

#[derive(Clone)]
pub struct BannerInfo {
    pub text: String,
    pub pkg: Option<String>,
    pub version: Option<String>,
}

/// The statement-level banner (the TS collectBanner): the LAST bang-block
/// comment in the gap between the prior statement's end and this
/// statement's start. Babel attaches a comment sitting between two
/// statements to the prior one's trailingComments or the next one's
/// leadingComments — the union of both buckets is exactly that gap, and
/// `[...trailing, ...leading].pop()` is its last bang-block in source
/// order.
fn collect_banner(
    source: &str,
    comments: &[oxc_ast::ast::Comment],
    gap_start: u32,
    stmt_start: u32,
) -> Option<BannerInfo> {
    let mut last: Option<(u32, String)> = None; // (span start, value)
    for comment in comments {
        if comment.is_line() {
            continue;
        }
        let span = comment.span();
        if span.start < gap_start || span.end > stmt_start {
            continue;
        }
        let Some(value) = bang_block_value(source, span) else {
            continue;
        };
        if last.as_ref().is_none_or(|(s, _)| span.start >= *s) {
            last = Some((span.start, value));
        }
    }
    let (_, raw) = last?;
    Some(parse_banner(&raw))
}

/// The bang-block banner inside a factory body (the TS
/// findBannerInsideBody): the FIRST bang-block whose package parses. The TS
/// walks the body DFS-with-a-stack (children in reverse push order); here
/// the scan is source order — the two agree unless one body carries MORE
/// THAN ONE bang-block where a later-in-source one parses first, which the
/// four-pair gate measures (deviation noted in the ledger claim).
fn find_banner_inside_body(
    source: &str,
    comments: &[oxc_ast::ast::Comment],
    body_span: Span,
) -> Option<BannerInfo> {
    for comment in comments {
        if comment.is_line() {
            continue;
        }
        let span = comment.span();
        if span.start < body_span.start || span.end > body_span.end {
            continue;
        }
        let Some(value) = bang_block_value(source, span) else {
            continue;
        };
        let info = parse_banner(&value);
        if info.pkg.is_some() {
            return Some(info);
        }
    }
    None
}

/// A block comment's value (between the delimiters) when it is a
/// bang-block: `isBangBlock` = CommentBlock whose value starts with `!`.
fn bang_block_value(source: &str, span: Span) -> Option<String> {
    let raw = &source[span.start as usize..span.end as usize];
    let value = raw.strip_prefix("/*")?.strip_suffix("*/")?;
    if !value.starts_with('!') {
        return None;
    }
    Some(value.to_string())
}

/// The TS parseBanner, exactly:
///   text = raw.replace(/^!/, '').trim()
///   match /^([@\w][@\w./_-]*)(?:\s+v?(\d[\w.+-]*))?(?:\s|$)/i
///   pkg  = match[1] minus trailing [.,_-]+
///   reject: empty after strip, a license-header false positive, and
///           one-word headers with no version and no shape.
pub fn parse_banner(raw: &str) -> BannerInfo {
    const FALSE_POSITIVES: [&str; 12] = [
        "copyright",
        "license",
        "licence",
        "mit",
        "bsd",
        "isc",
        "apache",
        "the",
        "this",
        "use",
        "see",
        "based",
    ];
    let text = raw.strip_prefix('!').unwrap_or(raw).trim();
    let no_match = BannerInfo {
        text: text.to_string(),
        pkg: None,
        version: None,
    };
    let Some(first) = text.chars().next() else {
        return no_match;
    };
    // `[@\w]` — the pkg's first char.
    if !(first == '@' || first.is_ascii_alphanumeric() || first == '_') {
        return no_match;
    }
    // `[@\w./_-]*` — the maximal run. Greedy with no useful backtrack: every
    // shorter run ends on a class char, which is neither `\s` nor `$`.
    let bytes = text.as_bytes();
    let mut pkg_end = first.len_utf8();
    while pkg_end < bytes.len()
        && (bytes[pkg_end].is_ascii_alphanumeric()
            || matches!(bytes[pkg_end], b'@' | b'.' | b'/' | b'_' | b'-'))
    {
        pkg_end += 1;
    }
    let raw_pkg = &text[..pkg_end];
    // The version attempt: `\s+v?(\d[\w.+-]*)` then `(?:\s|$)`. On ANY
    // failure of the version group the regex falls back to skipping it —
    // the match then requires `(?:\s|$)` directly after the pkg.
    let rest = &text[pkg_end..];
    if !rest.is_empty() && !is_js_ws(rest.as_bytes()[0]) {
        return no_match; // neither `\s` nor `$` — the whole regex fails
    }
    let mut version: Option<String> = None;
    let after_ws = rest.trim_start_matches(|c: char| {
        // JS `\s` (the ASCII subset a banner text carries — the TS trims
        // Unicode spaces too; non-ASCII whitespace in a banner is a
        // documented deviation the gate judges).
        matches!(c, ' ' | '\t' | '\n' | '\u{0b}' | '\u{0c}' | '\r')
    });
    let body = after_ws.strip_prefix('v').unwrap_or(after_ws);
    if body.as_bytes().first().is_some_and(|b| b.is_ascii_digit()) {
        let mut run = 1usize;
        while run < body.len()
            && (body.as_bytes()[run].is_ascii_alphanumeric()
                || matches!(body.as_bytes()[run], b'.' | b'+' | b'-'))
        {
            run += 1;
        }
        let after_run = &body[run..];
        if after_run.is_empty() || is_js_ws(after_run.as_bytes()[0]) {
            // match[2] EXCLUDES the `v` prefix.
            version = Some(body[..run].to_string());
        }
    }
    // pkg = raw_pkg minus trailing [.,_-]+
    let pkg = raw_pkg.trim_end_matches(['.', ',', '_', '-']);
    if pkg.is_empty() {
        return no_match;
    }
    if FALSE_POSITIVES.contains(&pkg.to_ascii_lowercase().as_str()) {
        return no_match;
    }
    // Require a scope/path, a hyphen, an interior dot, or an explicit
    // version — one-word headers like "Sharp" are not package banners.
    let has_shape =
        pkg.contains('/') || pkg.contains('-') || pkg.contains('.') || pkg.starts_with('@');
    if !has_shape && version.is_none() {
        return no_match;
    }
    BannerInfo {
        text: text.to_string(),
        pkg: Some(pkg.to_string()),
        version,
    }
}

/// The WP1.5 gate's dump: rebuild the TS modules.json's rows from a TS
/// dump's fresh text (the graph's classification anchors the text the
/// graph was built on). Migration scaffolding — deleted at phase 6 with
/// the TS core (02 §9).
pub mod modules_dump {
    use std::fs;
    use std::path::Path;

    use oxc_allocator::Allocator;
    use serde_json::{Value, json};

    use super::{classify_bun_modules, wrapper::find_wrapper_function};
    use crate::hash::serialize::SymbolTables;
    use crate::ingest::Ingest;

    pub fn dump_modules(ts_dump_dir: &Path, out_dir: &Path) -> Result<usize, String> {
        let meta_text = fs::read_to_string(ts_dump_dir.join("meta.json"))
            .map_err(|e| format!("meta.json: {e}"))?;
        let meta: Value = serde_json::from_str(&meta_text).map_err(|e| format!("meta: {e}"))?;

        // The classification runs TWICE in the TS pipeline: unpack-time on
        // the MINIFIED text (the vendor-naming one) and graph-time on the
        // FRESH text (the factory-body-skip one). Both are computed here the
        // same way; on real Bun bundles the graph site is None (the
        // beautifier splits the `{exports:{}}` marker across lines — ported
        // behavior, not an accident).
        let unpack = classify_text(ts_dump_dir, "minified.js")?;
        let graph = classify_text(ts_dump_dir, "fresh.js")?;
        if unpack.is_none() && graph.is_none() {
            // The TS writer emits no modules.json when neither site fired —
            // mirror the absence (a non-Bun input has no classification).
            return Ok(0);
        }
        let row_count = unpack
            .as_ref()
            .map(|(_, _, n)| *n)
            .or_else(|| graph.as_ref().map(|(_, _, n)| *n))
            .unwrap_or(0);

        fs::create_dir_all(out_dir).map_err(|e| format!("mkdir: {e}"))?;
        fs::write(
            out_dir.join("meta.json"),
            serde_json::to_string(&meta).unwrap(),
        )
        .map_err(|e| format!("write meta: {e}"))?;
        fs::write(
            out_dir.join("modules.json"),
            serde_json::to_string(&json!({
                "schemaVersion": 1,
                "unpack": site_json(unpack.as_ref().map(|(d, w, _)| (d, w.as_ref())), "minified"),
                "graph": site_json(graph.as_ref().map(|(d, w, _)| (d, w.as_ref())), "fresh"),
            }))
            .unwrap(),
        )
        .map_err(|e| format!("write modules: {e}"))?;
        Ok(row_count)
    }

    fn site_json(
        data: Option<(
            &super::BunModuleClassification,
            Option<&super::wrapper::WrapperFunction>,
        )>,
        label: &str,
    ) -> Value {
        let Some((data, wrapper)) = data else {
            return Value::Null;
        };
        let span = |s: oxc_span::Span| json!({"text": label, "start": s.start, "end": s.end});
        let mut rows: Vec<Value> = data
            .factories
            .iter()
            .map(|f| {
                let mut row = json!({
                    "key": span(f.span),
                    "factoryVar": f.factory_var,
                    "lineRange": [f.line_range.0, f.line_range.1],
                    "contentHash": f.content_hash,
                    "structuralHash": f.structural_hash,
                });
                let obj = row.as_object_mut().expect("row object");
                // The TS writes the banner fields only when present
                // (JSON.stringify drops undefined) — mirror the omission.
                if let Some(t) = &f.banner_text {
                    obj.insert("bannerText".into(), json!(t));
                }
                if let Some(p) = &f.banner_package {
                    obj.insert("bannerPackage".into(), json!(p));
                }
                if let Some(v) = &f.banner_version {
                    obj.insert("bannerVersion".into(), json!(v));
                }
                row
            })
            .collect();
        rows.sort_by(|a, b| {
            let key = |v: &Value| {
                (
                    v["key"]["start"].as_u64().unwrap_or(0),
                    v["key"]["end"].as_u64().unwrap_or(0),
                )
            };
            key(a).cmp(&key(b))
        });
        let wrapper_json = wrapper.map(|w| {
            json!({
                "span": {"text": label, "start": w.span.start, "end": w.span.end},
                "bodySpan": {"text": label, "start": w.body_span.start, "end": w.body_span.end},
                "bindingCount": w.binding_count,
            })
        });
        json!({
            "helperVar": data.helper_var,
            "wrapper": wrapper_json,
            "factories": rows,
        })
    }

    /// Classify one dump text (by file name) — None when no helper scan hit
    /// (a real Bun bundle's FRESH text, or a non-Bun input).
    fn classify_text(
        ts_dump_dir: &Path,
        file: &str,
    ) -> Result<
        Option<(
            super::BunModuleClassification,
            Option<super::wrapper::WrapperFunction>,
            usize,
        )>,
        String,
    > {
        let text = fs::read_to_string(ts_dump_dir.join("text").join(file))
            .map_err(|e| format!("{file}: {e}"))?;
        let allocator = Allocator::default();
        let ingest = Ingest::parse(&allocator, &text, "input.js");
        if !ingest.errors.is_empty() {
            return Err(format!(
                "oxc on {file}: {} diagnostic(s)",
                ingest.errors.len()
            ));
        }
        let wrapper = find_wrapper_function(ingest.program, ingest.semantic());
        let tables = SymbolTables::build(ingest.semantic());
        let classification = classify_bun_modules(
            &text,
            ingest.program,
            ingest.semantic(),
            wrapper.as_ref().map(|w| w.body_span),
            &tables,
        );
        let count = classification
            .as_ref()
            .map(|c| c.factories.len())
            .unwrap_or(0);
        Ok(classification.map(|c| (c, wrapper, count)))
    }
}

#[cfg(test)]
mod vendor_names_test;
