//! Pair a vendor module with its PREVIOUS VERSION by content (finding #90).
//!
//! A vendor module's name, file and internal identifier carry across
//! releases only on an EXACT structural-hash match
//! ([`super::name_cjs_factories`]), and the hash keeps string LENGTHS — so
//! one edited string of a different length makes an unchanged library look
//! new: it is re-asked of the model, gets a new name, a new file and a new
//! identifier in every app file that uses it. On the eval's hops that was
//! 167 modules per run, 141 of them with a real twin in the prior tree
//! (/work/vendor-mapping-2026-10-06/REPORT.txt).
//!
//! This is the measured matcher of that investigation (`pairsim3.py`),
//! ported rule for rule. Two features per module, over the module's
//! factory text:
//!
//! - **shingle** — the Jaccard similarity of the sets of 5-token runs, over
//!   tokens with minified-looking identifiers (`lib_<hash8>`, or at most 4
//!   characters) masked to one token, and a long string (> 40 characters)
//!   split into its words, so a prose edit costs only the words it changed;
//! - **literal** — an IDF-weighted Jaccard of the module's string VALUES
//!   (5+ characters) and its longer identifiers (5+ characters), the IDF
//!   taken over every module of the WHOLE prior tree (a literal the prior
//!   tree never saw weighs [`UNSEEN_LITERAL_IDF`]).
//!
//! The score is the higher of the two. A pair is ACCEPTED only when it is
//! each side's best (1:1, mutual), scores at least [`MIN_SCORE`], and beats
//! the runner-up on BOTH sides by at least [`MIN_MARGIN`] — anything less
//! decided is left to the model, exactly as before. Deterministic: no model
//! call, no ordering dependence (a tie can never clear the margin).
//!
//! Measured on 5 eval runs x 3 hops (prior = the run's own base): 104 pairs
//! per run, 0 wrong of 475 checked against an independent truth (the
//! grammar modules' own name field) and 40 of 40 right in a hand-judged
//! sample.

use std::collections::HashMap;

use super::lex::{is_string_token, lex, unquoted};
use super::{FactoryRecord, NameSource};
use crate::similarity::{jaccard_from_counts, weighted_jaccard};

/// The minimum score a pair must reach.
pub const MIN_SCORE: f64 = 0.3;
/// How far the accepted pair must beat the runner-up, on BOTH sides.
pub const MIN_MARGIN: f64 = 0.1;
/// Shingle width, in tokens.
const SHINGLE: usize = 5;
/// A string literal longer than this (in characters, quotes included)
/// is split into its words for the shingles.
const LONG_STRING: usize = 40;
/// The IDF weight of a literal no prior module holds.
pub const UNSEEN_LITERAL_IDF: f64 = 8.0;

/// A string literal's words (`\w+` runs and single other non-space chars).
fn words(text: &str, out: &mut Vec<String>) {
    let mut run = String::new();
    for c in text.chars() {
        if c.is_alphanumeric() || c == '_' {
            run.push(c);
            continue;
        }
        if !run.is_empty() {
            out.push(std::mem::take(&mut run));
        }
        if !c.is_whitespace() {
            out.push(c.to_string());
        }
    }
    if !run.is_empty() {
        out.push(run);
    }
}

// ---------------------------------------------------------------------------
// Features
// ---------------------------------------------------------------------------

/// One module's two feature sets.
#[derive(Clone, Debug, Default)]
pub struct ModuleFeatures {
    /// The 5-token run hashes, sorted and unique.
    shingles: Vec<u64>,
    /// String values and longer identifiers (`#`-prefixed), sorted, unique.
    literals: Vec<String>,
    /// How many tokens the module lexed to (the size the shingles see).
    pub tokens: usize,
}

/// FNV-1a over one run of tokens — deterministic across runs and builds.
fn shingle_hash(run: &[String]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for t in run {
        for b in t.bytes().chain(std::iter::once(0xff)) {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
    }
    h
}

/// The features of one module's factory text.
pub fn module_features(text: &str) -> ModuleFeatures {
    let tokens = lex(text);
    let mut literals: Vec<String> = tokens
        .iter()
        .filter_map(|t| {
            let first = t.chars().next()?;
            if is_string_token(t) {
                (t.chars().count() >= 5).then(|| unquoted(t).to_string())
            } else if t.chars().count() >= 5
                && (first.is_alphabetic() || first == '_' || first == '$')
            {
                Some(format!("#{t}"))
            } else {
                None
            }
        })
        .collect();
    literals.sort_unstable();
    literals.dedup();
    let mut features: Vec<String> = Vec::with_capacity(tokens.len());
    for t in &tokens {
        if is_string_token(t) && t.chars().count() > LONG_STRING {
            features.push("<str>".to_string());
            words(unquoted(t), &mut features);
            features.push("</str>".to_string());
        } else {
            features.push(t.clone());
        }
    }
    let mut shingles: Vec<u64> = crate::similarity::shingle_windows(&features, SHINGLE)
        .map(shingle_hash)
        .collect();
    shingles.sort_unstable();
    shingles.dedup();
    ModuleFeatures {
        shingles,
        literals,
        tokens: tokens.len(),
    }
}

/// The spelling every binding takes in [`blind_text`]: a short name, so the
/// lexer masks it like a minified one.
const BLIND_BINDING: &str = "_";

/// A factory's text read BLIND to the spelling of its own bindings
/// (exp173): formatted by the pipeline's formatter, then every identifier
/// that declares or reads a binding of the factory itself spelled
/// [`BLIND_BINDING`] — a shorthand property as its expansion
/// (`{load}` → `{load: _}`), the way a rename writes it. Free names (the
/// references to other modules, `require`, globals), property names and
/// literals are kept.
///
/// A names-mode prior tree holds its vendored bodies NAMED by the pipeline
/// (exp169, `naming::vendor_bodies`) and formatted, while the fresh factory
/// is the bundle's raw text: read as written, the pipeline's own names hide
/// a changed module's twin. Both sides read blind are the same text up to
/// the code's real edits. The pipeline names bindings only, so nothing it
/// assigns survives here. Text the formatter refuses is masked unformatted;
/// text that does not parse is returned as given (its old reading).
pub fn blind_text(text: &str) -> String {
    let statement = crate::naming::vendor_bodies::join_bodies([text]);
    let formatted = crate::format::format_file(&statement, &Default::default(), &[])
        .map(|f| f.text)
        .unwrap_or(statement);
    mask_bindings(&formatted).unwrap_or(formatted)
}

/// `code` with every binding identifier, and every reference that resolves
/// to one, spelled [`BLIND_BINDING`]. None when it does not parse.
fn mask_bindings(code: &str) -> Option<String> {
    use oxc_ast::AstKind;
    let allocator = oxc_allocator::Allocator::default();
    let ingest = crate::ingest::Ingest::parse_unambiguous(&allocator, code);
    if !ingest.errors.is_empty() {
        return None;
    }
    let semantic = ingest.semantic();
    let scoping = semantic.scoping();
    let nodes = semantic.nodes();
    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    for node in nodes.iter() {
        let (span, name) = match node.kind() {
            AstKind::BindingIdentifier(b) => (b.span, b.name.as_str()),
            AstKind::IdentifierReference(r)
                if r.reference_id
                    .get()
                    .is_some_and(|id| scoping.get_reference(id).symbol_id().is_some()) =>
            {
                (r.span, r.name.as_str())
            }
            _ => continue,
        };
        let spelled = if crate::emit::cjs::babel::is_shorthand_value(nodes, node.id(), span) {
            format!("{name}: {BLIND_BINDING}")
        } else {
            BLIND_BINDING.to_string()
        };
        edits.push((span.start as usize, span.end as usize, spelled));
    }
    edits.sort_unstable_by_key(|e| e.0);
    edits.dedup_by_key(|e| e.0);
    let mut out = String::with_capacity(code.len());
    let mut at = 0;
    for (start, end, spelled) in edits {
        if start < at {
            continue;
        }
        out.push_str(&code[at..start]);
        out.push_str(&spelled);
        at = end;
    }
    out.push_str(&code[at..]);
    Some(out)
}

/// [`module_features`] of the [`blind_text`]: the reading for a prior tree
/// whose vendored bodies the pipeline named.
pub fn blind_module_features(text: &str) -> ModuleFeatures {
    module_features(&blind_text(text))
}

/// `log((N + 1) / (df + 0.5))` per literal, over every module of the prior
/// tree.
pub fn literal_idf<'a>(modules: impl Iterator<Item = &'a ModuleFeatures>) -> HashMap<String, f64> {
    let mut df: HashMap<&str, usize> = HashMap::new();
    let mut n = 0usize;
    for m in modules {
        n += 1;
        for l in &m.literals {
            *df.entry(l.as_str()).or_default() += 1;
        }
    }
    df.into_iter()
        .map(|(l, c)| (l.to_string(), ((n as f64 + 1.0) / (c as f64 + 0.5)).ln()))
        .collect()
}

// ---------------------------------------------------------------------------
// The pairing
// ---------------------------------------------------------------------------

/// One accepted pair, with the evidence it was accepted on.
#[derive(Clone, Debug, PartialEq)]
pub struct ContentPair {
    /// Index into the fresh candidates / the prior leftovers.
    pub fresh: usize,
    pub prior: usize,
    pub score: f64,
    pub shingle: f64,
    pub literal: f64,
    /// The score minus the runner-up's, on each side.
    pub fresh_margin: f64,
    pub prior_margin: f64,
}

/// Every candidate x leftover score: (max, shingle, literal), row-major.
struct Scores {
    cols: usize,
    cells: Vec<(f64, f64, f64)>,
}

impl Scores {
    fn at(&self, f: usize, p: usize) -> (f64, f64, f64) {
        self.cells[f * self.cols + p]
    }
}

fn weight(idf: &HashMap<String, f64>, l: &str) -> f64 {
    idf.get(l).copied().unwrap_or(UNSEEN_LITERAL_IDF)
}

fn score_all(
    fresh: &[ModuleFeatures],
    prior: &[ModuleFeatures],
    idf: &HashMap<String, f64>,
) -> Scores {
    // Inverted indexes over the prior side: a pair that shares nothing
    // scores 0 and is never visited.
    let mut by_shingle: HashMap<u64, Vec<u32>> = HashMap::new();
    let mut by_literal: HashMap<&str, Vec<u32>> = HashMap::new();
    for (p, m) in prior.iter().enumerate() {
        for &s in &m.shingles {
            by_shingle.entry(s).or_default().push(p as u32);
        }
        for l in &m.literals {
            by_literal.entry(l.as_str()).or_default().push(p as u32);
        }
    }
    let prior_weight: Vec<f64> = prior
        .iter()
        .map(|m| m.literals.iter().map(|l| weight(idf, l)).sum())
        .collect();
    let rows = crate::par::map_ordered(fresh, |f| {
        let mut shared = vec![0usize; prior.len()];
        for s in &f.shingles {
            for &p in by_shingle.get(s).map_or(&[][..], Vec::as_slice) {
                shared[p as usize] += 1;
            }
        }
        let mut shared_weight = vec![0f64; prior.len()];
        for l in &f.literals {
            let w = weight(idf, l);
            for &p in by_literal.get(l.as_str()).map_or(&[][..], Vec::as_slice) {
                shared_weight[p as usize] += w;
            }
        }
        let own_weight: f64 = f.literals.iter().map(|l| weight(idf, l)).sum();
        (0..prior.len())
            .map(|p| {
                let shingle =
                    jaccard_from_counts(shared[p], f.shingles.len(), prior[p].shingles.len());
                let literal = weighted_jaccard(shared_weight[p], own_weight, prior_weight[p]);
                (shingle.max(literal), shingle, literal)
            })
            .collect::<Vec<_>>()
    });
    Scores {
        cols: prior.len(),
        cells: rows.into_iter().flatten().collect(),
    }
}

/// The best and second-best score over `scores` (first index wins a tie —
/// a tie never clears the margin, so which one is never decisive).
fn best_two(scores: impl Iterator<Item = f64>) -> Option<(usize, f64, f64)> {
    let mut best: Option<(usize, f64)> = None;
    let mut second = 0.0f64;
    for (i, s) in scores.enumerate() {
        match best {
            Some((_, b)) if s <= b => second = second.max(s),
            Some((_, b)) => {
                second = second.max(b);
                best = Some((i, s));
            }
            None => best = Some((i, s)),
        }
    }
    best.map(|(i, b)| (i, b, second))
}

/// The accepted 1:1 pairs between `fresh` candidates and `prior`
/// leftovers, in fresh order.
pub fn pair_by_content(
    fresh: &[ModuleFeatures],
    prior: &[ModuleFeatures],
    idf: &HashMap<String, f64>,
) -> Vec<ContentPair> {
    if fresh.is_empty() || prior.is_empty() {
        return Vec::new();
    }
    let scores = score_all(fresh, prior, idf);
    let column_best: Vec<(usize, f64, f64)> = (0..prior.len())
        .map(|p| best_two((0..fresh.len()).map(|f| scores.at(f, p).0)).expect("non-empty"))
        .collect();
    let mut pairs = Vec::new();
    for f in 0..fresh.len() {
        let (p, score, row_second) =
            best_two((0..prior.len()).map(|p| scores.at(f, p).0)).expect("non-empty");
        let (col_f, _, col_second) = column_best[p];
        let accepted = col_f == f
            && score >= MIN_SCORE
            && score - row_second >= MIN_MARGIN
            && score - col_second >= MIN_MARGIN;
        if accepted {
            let (_, shingle, literal) = scores.at(f, p);
            pairs.push(ContentPair {
                fresh: f,
                prior: p,
                score,
                shingle,
                literal,
                fresh_margin: score - row_second,
                prior_margin: score - col_second,
            });
        }
    }
    pairs
}

// ---------------------------------------------------------------------------
// The carry
// ---------------------------------------------------------------------------

/// One prior manifest entry, as the content carry reads it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PriorModule {
    /// The prior tree's file (relative to its root).
    pub file_name: String,
    pub name: String,
    /// The prior manifest's `nameSource` label (kept by a carry, #71).
    pub origin: NameSource,
    /// The structural hash in THIS run's bytes — a stale-era entry's is the
    /// re-keyed hash, or one that can never match ([`super::vendor_content`]).
    pub structural_hash: String,
    pub runtime_identifier: Option<String>,
    /// The prior record's declared identity (finding #95) — what the
    /// declared pairing keys on. None for a module that declared nothing,
    /// or a record written before identities were recorded.
    pub identity: Option<super::identity::DeclaredIdentity>,
}

/// What a content pair carries onto the fresh record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CarriedIdentity {
    /// The prior file, kept when it is a vendor file (`vendor/…/x.js`);
    /// None for a pair across the vendor/app-asset line, which carries
    /// only the identifier.
    pub file_name: Option<String>,
    pub runtime_identifier: Option<String>,
}

/// What one content pair carried.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Carried {
    /// The name, its label, the file and the identifier (a vendor module).
    Identity,
    /// The identifier only (an app text asset, or a prior asset).
    Identifier,
}

/// One carry, for the run's diagnostics.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentCarry {
    /// The fresh factory's position in the classification.
    #[serde(skip)]
    pub record: usize,
    pub structural_hash: String,
    pub prior_file: String,
    pub prior_name: String,
    pub carried: Carried,
    pub score: f64,
    pub shingle: f64,
    pub literal: f64,
    pub fresh_margin: f64,
    pub prior_margin: f64,
    /// Filled once the file names are chosen.
    pub file_name: String,
}

/// The fresh records the content carry may pair: named by nothing
/// deterministic (the cascade's fallbacks, which would go to the model) or
/// app text assets (named from their own text, but their identifier is
/// worth keeping).
pub fn is_content_candidate(record: &FactoryRecord) -> bool {
    matches!(
        record.name_source,
        Some(NameSource::Fallback | NameSource::Asset)
    )
}

/// A prior entry nothing carried into: its hash is no fresh factory's.
pub fn leftover_priors<'p>(
    prior: &'p [PriorModule],
    factories: &[FactoryRecord],
) -> Vec<&'p PriorModule> {
    let fresh: std::collections::HashSet<&str> = factories
        .iter()
        .map(|f| f.structural_hash.as_str())
        .collect();
    prior
        .iter()
        .filter(|p| !fresh.contains(p.structural_hash.as_str()))
        .collect()
}

/// A prior path a carry may reuse: `vendor/<safe segments>.js`.
pub fn reusable_vendor_path(file_name: &str, vendor_dir: &str) -> bool {
    let Some(rest) = file_name
        .strip_prefix(vendor_dir)
        .and_then(|r| r.strip_prefix('/'))
        .and_then(|r| r.strip_suffix(".js"))
    else {
        return false;
    };
    !rest.is_empty()
        && rest.split('/').all(|seg| {
            !seg.is_empty()
                && seg != "."
                && seg != ".."
                && super::vendor_names::sanitize_fs_name(seg) == seg
        })
}

/// Apply accepted pairs to the fresh records: a vendor module takes the
/// prior's name, its label, its file and its identifier (run source
/// "content-pair" — never re-asked); an app text asset, or a pair whose
/// prior was an asset, takes the identifier only. Returns one carry per
/// pair, for the diagnostics.
pub fn apply_content_pairs(
    factories: &mut [FactoryRecord],
    candidates: &[usize],
    leftovers: &[&PriorModule],
    pairs: &[ContentPair],
    vendor_dir: &str,
) -> Vec<ContentCarry> {
    pairs
        .iter()
        .map(|pair| {
            let record = candidates[pair.fresh];
            let prior = leftovers[pair.prior];
            let factory = &mut factories[record];
            let carried = carry_from_prior(factory, prior, NameSource::ContentPair, vendor_dir);
            ContentCarry {
                record,
                structural_hash: factory.structural_hash.clone(),
                prior_file: prior.file_name.clone(),
                prior_name: prior.name.clone(),
                carried,
                score: pair.score,
                shingle: pair.shingle,
                literal: pair.literal,
                fresh_margin: pair.fresh_margin,
                prior_margin: pair.prior_margin,
                file_name: String::new(),
            }
        })
        .collect()
}

/// THE carry of one pair (content or declared): a fallback vendor module
/// takes the prior's name, its label, its file and its identifier, under
/// run source `via` (never re-asked); an app text asset, or a pair whose
/// prior was an asset, takes the identifier only.
pub fn carry_from_prior(
    factory: &mut FactoryRecord,
    prior: &PriorModule,
    via: NameSource,
    vendor_dir: &str,
) -> Carried {
    let whole = factory.name_source == Some(NameSource::Fallback)
        && prior.origin != NameSource::Asset
        && reusable_vendor_path(&prior.file_name, vendor_dir);
    if whole {
        factory.name = Some(prior.name.clone());
        factory.name_source = Some(via);
        factory.name_origin = Some(prior.origin);
    }
    factory.carried = Some(CarriedIdentity {
        file_name: whole.then(|| prior.file_name.clone()),
        runtime_identifier: prior.runtime_identifier.clone(),
    });
    if whole {
        Carried::Identity
    } else {
        Carried::Identifier
    }
}

/// One declared pair's carry, for the run's diagnostics.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeclaredCarry {
    #[serde(skip)]
    pub record: usize,
    pub structural_hash: String,
    pub prior_file: String,
    pub prior_name: String,
    /// The declared key both sides share (`package/member`, case-folded).
    pub key: String,
    pub carried: Carried,
    /// Filled once the file names are chosen.
    pub file_name: String,
}

/// The declared-identity pairing (finding #95), run after the exact-hash
/// carry and before the content pairing: every fallback module with a
/// declared identity against every prior module nothing carried into, paired
/// when the key is equal and unique on both sides
/// ([`super::identity::pair_by_declared_identity`]). Returns the carries.
pub fn carry_by_declared_identity(
    factories: &mut [FactoryRecord],
    leftovers: &[&PriorModule],
    vendor_dir: &str,
) -> Vec<DeclaredCarry> {
    let candidates: Vec<usize> = (0..factories.len())
        .filter(|&i| {
            factories[i].name_source == Some(NameSource::Fallback)
                && factories[i].identity.is_some()
        })
        .collect();
    let fresh: Vec<_> = candidates
        .iter()
        .map(|&i| factories[i].identity.as_ref())
        .collect();
    let prior: Vec<_> = leftovers.iter().map(|p| p.identity.as_ref()).collect();
    let pairs = super::identity::pair_by_declared_identity(&fresh, &prior);
    pairs
        .into_iter()
        .map(|pair| {
            let record = candidates[pair.fresh];
            let prior = leftovers[pair.prior];
            let factory = &mut factories[record];
            let carried = carry_from_prior(factory, prior, NameSource::DeclaredPair, vendor_dir);
            DeclaredCarry {
                record,
                structural_hash: factory.structural_hash.clone(),
                prior_file: prior.file_name.clone(),
                prior_name: prior.name.clone(),
                key: pair.key,
                carried,
                file_name: String::new(),
            }
        })
        .collect()
}

#[cfg(test)]
#[path = "vendor_pairing/vendor_pairing_test.rs"]
mod vendor_pairing_test;
