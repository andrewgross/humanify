//! Pair a vendor module with its PREVIOUS VERSION by content (finding #88).
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

use super::{FactoryRecord, NameSource};

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

// ---------------------------------------------------------------------------
// The lexer (lex.py): strings, templates, regex literals, identifiers,
// numbers, punctuation; comments and whitespace dropped. Good enough for
// similarity, never for parsing.
// ---------------------------------------------------------------------------

const KEYWORDS: [&str; 40] = [
    "var",
    "let",
    "const",
    "function",
    "return",
    "if",
    "else",
    "for",
    "while",
    "do",
    "switch",
    "case",
    "break",
    "continue",
    "new",
    "this",
    "typeof",
    "instanceof",
    "in",
    "of",
    "void",
    "delete",
    "throw",
    "try",
    "catch",
    "finally",
    "class",
    "extends",
    "super",
    "null",
    "true",
    "false",
    "undefined",
    "async",
    "await",
    "yield",
    "get",
    "set",
    "static",
    "default",
];

/// The previous tokens after which a `/` starts a regex literal.
const REGEX_AFTER_WORDS: [&str; 14] = [
    "return",
    "typeof",
    "case",
    "do",
    "else",
    "in",
    "of",
    "new",
    "delete",
    "void",
    "throw",
    "instanceof",
    "yield",
    "await",
];
const REGEX_AFTER_CHARS: &str = "(,=:[!&|?{};+-*%<>~^";
const REGEX_AFTER_OPS: [&str; 10] = ["=>", "&&", "||", "??", "==", "===", "!=", "!==", "+=", "-="];

/// Multi-character punctuators, in the order the alternation tries them.
const PUNCTUATORS: [&str; 29] = [
    ">>>=", "...", "===", "!==", "**=", "<<=", ">>=", ">>>", "=>", "==", "!=", "<=", ">=", "&&",
    "||", "??", "?.", "++", "--", "+=", "-=", "*=", "/=", "%=", "&=", "|=", "^=", "<<", ">>",
];
const PUNCTUATOR_CHARS: &str = "{}()[];,<>+-*/%&|^!~?:=.@#";

fn is_id_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_' || c == '$' || ('\u{80}'..='\u{ffff}').contains(&c)
}

fn is_id_part(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '$' || ('\u{80}'..='\u{ffff}').contains(&c)
}

/// Minified-looking: `lib_<8 hex>` (optionally `_<n>`), or 1-4 identifier
/// characters starting with an ASCII letter, `_` or `$`.
fn is_minified(t: &str) -> bool {
    if let Some(rest) = t.strip_prefix("lib_")
        && rest.len() >= 8
        && rest.is_char_boundary(8)
    {
        let (hex, tail) = rest.split_at(8);
        let hex_ok = hex
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
        let tail_ok = tail.is_empty()
            || tail
                .strip_prefix('_')
                .is_some_and(|d| !d.is_empty() && d.chars().all(|c| c.is_ascii_digit()));
        if hex_ok && tail_ok {
            return true;
        }
    }
    let mut chars = t.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    let head_ok = first.is_ascii_alphabetic() || first == '_' || first == '$';
    let rest: Vec<char> = chars.collect();
    head_ok
        && rest.len() <= 3
        && rest
            .iter()
            .all(|&c| c.is_alphanumeric() || c == '_' || c == '$')
}

fn regex_may_follow(prev: Option<&str>) -> bool {
    let Some(p) = prev else {
        return true;
    };
    REGEX_AFTER_WORDS.contains(&p)
        || (p.chars().count() == 1 && REGEX_AFTER_CHARS.contains(p))
        || REGEX_AFTER_OPS.contains(&p)
}

fn starts_with_at(s: &[char], i: usize, pat: &str) -> bool {
    pat.chars()
        .enumerate()
        .all(|(k, c)| s.get(i + k) == Some(&c))
}

fn text_of(s: &[char], from: usize, to: usize) -> String {
    s[from.min(s.len())..to.min(s.len())].iter().collect()
}

/// The end of a quoted string starting at `i` (one past the closing quote,
/// clamped to the text).
fn string_end(s: &[char], i: usize) -> usize {
    let quote = s[i];
    let mut j = i + 1;
    while j < s.len() && s[j] != quote {
        j += if s[j] == '\\' { 2 } else { 1 };
    }
    j + 1
}

fn template_end(s: &[char], i: usize) -> usize {
    let mut j = i + 1;
    let mut depth = 0usize;
    while j < s.len() {
        if s[j] == '\\' {
            j += 2;
            continue;
        }
        if depth == 0 && s[j] == '`' {
            break;
        }
        if starts_with_at(s, j, "${") {
            depth += 1;
            j += 2;
            continue;
        }
        if depth > 0 && s[j] == '}' {
            depth -= 1;
        }
        j += 1;
    }
    j + 1
}

fn regex_end(s: &[char], i: usize) -> usize {
    let mut j = i + 1;
    let mut class = false;
    while j < s.len() && s[j] != '\n' {
        match s[j] {
            '\\' => {
                j += 2;
                continue;
            }
            '[' => class = true,
            ']' => class = false,
            '/' if !class => break,
            _ => {}
        }
        j += 1;
    }
    j += 1;
    while j < s.len() && s[j].is_alphabetic() {
        j += 1;
    }
    j
}

/// lex.py's NUM: hex/binary/octal, decimal with fraction and exponent, or
/// a leading-dot fraction; an optional BigInt `n`. None when nothing
/// matches (the char then lexes as punctuation).
fn number_end(s: &[char], i: usize) -> Option<usize> {
    let digits = |mut j: usize, allow_underscore: bool| {
        while j < s.len() && (s[j].is_ascii_digit() || (allow_underscore && s[j] == '_')) {
            j += 1;
        }
        j
    };
    let exponent = |j: usize| -> usize {
        if j < s.len() && (s[j] == 'e' || s[j] == 'E') {
            let mut k = j + 1;
            if k < s.len() && (s[k] == '+' || s[k] == '-') {
                k += 1;
            }
            let end = digits(k, false);
            if end > k {
                return end;
            }
        }
        j
    };
    let mut j;
    if s[i] == '0' && matches!(s.get(i + 1), Some('x' | 'X' | 'b' | 'B' | 'o' | 'O')) && {
        let k = i + 2;
        k < s.len() && (s[k].is_ascii_hexdigit() || s[k] == '_')
    } {
        j = i + 2;
        while j < s.len() && (s[j].is_ascii_hexdigit() || s[j] == '_') {
            j += 1;
        }
    } else if s[i].is_ascii_digit() {
        j = digits(i + 1, true);
        if j < s.len() && s[j] == '.' {
            j = digits(j + 1, false);
        }
        j = exponent(j);
    } else if s[i] == '.' && s.get(i + 1).is_some_and(|c| c.is_ascii_digit()) {
        j = digits(i + 1, false);
        j = exponent(j);
    } else {
        return None;
    }
    if j < s.len() && s[j] == 'n' {
        j += 1;
    }
    Some(j)
}

fn punctuator_end(s: &[char], i: usize) -> Option<usize> {
    if let Some(p) = PUNCTUATORS.iter().find(|p| starts_with_at(s, i, p)) {
        return Some(i + p.chars().count());
    }
    PUNCTUATOR_CHARS.contains(s[i]).then_some(i + 1)
}

/// The token stream of `text`, minified-looking identifiers masked to `I`.
pub fn lex(text: &str) -> Vec<String> {
    let s: Vec<char> = text.chars().collect();
    let n = s.len();
    let mut out: Vec<String> = Vec::new();
    // The previous significant token, RAW (unmasked).
    let mut prev: Option<String> = None;
    let mut i = 0usize;
    while i < n {
        let c = s[i];
        if matches!(c, ' ' | '\t' | '\r' | '\n') {
            i += 1;
            continue;
        }
        if starts_with_at(&s, i, "//") {
            i = (i..n).find(|&j| s[j] == '\n').unwrap_or(n);
            continue;
        }
        if starts_with_at(&s, i, "/*") {
            i = (i + 2..n.saturating_sub(1))
                .find(|&j| s[j] == '*' && s[j + 1] == '/')
                .map_or(n, |j| j + 2);
            continue;
        }
        let end = match c {
            '"' | '\'' => Some(string_end(&s, i)),
            '`' => Some(template_end(&s, i)),
            '/' if regex_may_follow(prev.as_deref()) => Some(regex_end(&s, i)),
            _ => None,
        };
        if let Some(end) = end {
            let tok = text_of(&s, i, end);
            prev = Some(tok.clone());
            out.push(tok);
            i = end;
            continue;
        }
        if is_id_start(c) {
            let mut j = i + 1;
            while j < n && is_id_part(s[j]) {
                j += 1;
            }
            let tok = text_of(&s, i, j);
            out.push(if !KEYWORDS.contains(&tok.as_str()) && is_minified(&tok) {
                "I".to_string()
            } else {
                tok.clone()
            });
            prev = Some(tok);
            i = j;
            continue;
        }
        let end = if c.is_ascii_digit() || c == '.' {
            number_end(&s, i)
        } else {
            None
        }
        .or_else(|| punctuator_end(&s, i))
        .unwrap_or(i + 1);
        let tok = text_of(&s, i, end);
        prev = Some(tok.clone());
        out.push(tok);
        i = end;
    }
    out
}

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

fn is_string_token(t: &str) -> bool {
    matches!(t.chars().next(), Some('"' | '\'' | '`'))
}

/// The inside of a quoted token (its first and last character dropped).
fn unquoted(t: &str) -> &str {
    let mut chars = t.char_indices();
    let start = chars.next().map_or(0, |(_, c)| c.len_utf8());
    let end = t
        .char_indices()
        .last()
        .map_or(t.len(), |(i, _)| i)
        .max(start);
    &t[start..end]
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
    let mut shingles: Vec<u64> = if features.len() < SHINGLE {
        vec![shingle_hash(&features)]
    } else {
        features.windows(SHINGLE).map(shingle_hash).collect()
    };
    shingles.sort_unstable();
    shingles.dedup();
    ModuleFeatures {
        shingles,
        literals,
        tokens: tokens.len(),
    }
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
                let union = f.shingles.len() + prior[p].shingles.len() - shared[p];
                let shingle = if union == 0 {
                    0.0
                } else {
                    shared[p] as f64 / union as f64
                };
                let wunion = own_weight + prior_weight[p] - shared_weight[p];
                let literal = if wunion > 0.0 {
                    shared_weight[p] / wunion
                } else {
                    0.0
                };
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
            let whole = factory.name_source == Some(NameSource::Fallback)
                && prior.origin != NameSource::Asset
                && reusable_vendor_path(&prior.file_name, vendor_dir);
            if whole {
                factory.name = Some(prior.name.clone());
                factory.name_source = Some(NameSource::ContentPair);
                factory.name_origin = Some(prior.origin);
            }
            factory.carried = Some(CarriedIdentity {
                file_name: whole.then(|| prior.file_name.clone()),
                runtime_identifier: prior.runtime_identifier.clone(),
            });
            ContentCarry {
                record,
                structural_hash: factory.structural_hash.clone(),
                prior_file: prior.file_name.clone(),
                prior_name: prior.name.clone(),
                carried: if whole {
                    Carried::Identity
                } else {
                    Carried::Identifier
                },
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

#[cfg(test)]
#[path = "vendor_pairing/vendor_pairing_test.rs"]
mod vendor_pairing_test;
