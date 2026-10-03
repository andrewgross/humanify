//! The naming floor's name-shape predicates (the part of
//! `src/rename/minted-census.ts` validated rename reads: the carried-names
//! rule records a binding when an APPLIED name is below the floor). WP4.5
//! owns the rest of minted-census.ts and extends this module.
//!
//! Pinned to the TS by `test/parity/wp31-names.json` — post-cutover the
//! pin is OURS to re-cut (the TS that froze it is deleted); the letters'
//! rows were re-cut 2026-09-30 with the single-letter decision below. Two
//! JS-isms carried over on purpose: `name.length` counts UTF-16 code units
//! (so `é` is a length-1 name), and `toLowerCase()` is Unicode
//! lowercasing.
//!
//! TWO distinct questions live here, and every consumer must know which
//! one it is asking (docs/responsibility.md, the `rename::floor` row):
//!
//! - WHO GETS ASKED — since 2026-09-30 (Andrew's provenance decision)
//!   this is NOT a name-shape question for the coverage sweep anymore:
//!   the sweep targets bindings by LEDGER (renamed? asked? exhausted? —
//!   `rename::validated`'s decision ledger, joined by name across the
//!   text boundary), and the ONLY shape exception left in its target
//!   path is [`is_convention_carveout`] (`_`, `__`, `$`-only: deliberate
//!   placeholders, not un-renamed code). [`is_bun_token`] still answers
//!   the shape question where shape IS the question: the minted-census
//!   METER's population, the class-id floor's derivation filter, vote
//!   candidacy, and the below-floor/carried rules.
//! - WHAT ANSWER MAY LAND — may the model's suggestion become the new
//!   name? ([`is_sweep_answer_acceptable`]: the sweep's answer filter.)
//!   This stays a STRING question by nature ("is this candidate junk?"),
//!   unchanged by the provenance decision: a single letter passes, junk
//!   shapes (`a1b`, `_`-tails, `$`) stay refused.
//!   Since 2026-10-03 its BORROWED-STEM half ([`borrowed_minified_stem`]:
//!   `H6tClass` wears the program's minified `H6t`) is asked by EVERY
//!   LLM naming site — the wave barrier and the sweep alike. The shape
//!   half stays the sweep's alone, a declared difference: replayed over
//!   the main pass's recorded answers it would refuse `iv`, `fd`, `x1`,
//!   `is2017OrLater`, `LZ77Compressor` (461 answers, mostly real names).

/// Short words that are real names, not mints (`SHORT_WORDS`). The ten
/// single letters a/b/e/i/j/k/n/t/x/y left this list 2026-09-30 (Andrew:
/// "remove the limit on single character names not being processed" —
/// `is_bun_token` treats them as minted now); the real two/three-letter
/// words stay.
const SHORT_WORDS: &[&str] = &[
    "abs", "add", "arg", "cb", "col", "ctx", "cwd", "db", "del", "dir", "end", "env", "err", "ext",
    "fn", "fs", "get", "gid", "go", "has", "id", "idx", "io", "ip", "key", "len", "log", "map",
    "max", "min", "msg", "now", "num", "obj", "ok", "os", "out", "pid", "pos", "raw", "req", "res",
    "row", "run", "sep", "set", "str", "sum", "tag", "ui", "uid", "url", "val",
];

/// Domain stems a mint-shaped head is allowed to carry (`DOMAIN_STEMS`).
const DOMAIN_STEMS: &[&str] = &[
    "e164", "ec2", "s3", "sha1", "sha256", "sha512", "md5", "utf8", "utf16", "base64", "http2",
    "oauth2", "i18n", "l10n", "a11y", "es5", "es6", "es2015", "ipv4", "ipv6", "v8", "w3c", "k8s",
    "b64", "u2f", "x509",
];

/// Stems that count only with a word tail (`SUFFIX_REQUIRED_STEMS`).
const SUFFIX_REQUIRED_STEMS: &[&str] = &["h1", "h2", "h3", "h4", "h5", "h6", "it2", "v1", "x0"];

/// UTF-16 length (JS `name.length`).
fn js_len(name: &str) -> usize {
    name.encode_utf16().count()
}

/// The UTF-16 unit at `index` (JS `name[index]`), when it is ASCII.
fn js_char_at(name: &str, index: usize) -> Option<u16> {
    name.encode_utf16().nth(index)
}

/// `isWordTailBoundary`: `_` or an ASCII capital.
fn is_word_tail_boundary(next: Option<u16>) -> bool {
    next.is_some_and(|c| c == u16::from(b'_') || (u16::from(b'A')..=u16::from(b'Z')).contains(&c))
}

/// `CONSTANT_CASE`: `/^[A-Z][A-Z0-9]*(?:_[A-Z0-9]+)+$/`.
fn is_constant_case(name: &str) -> bool {
    let parts: Vec<&str> = name.split('_').collect();
    parts.len() >= 2
        && name.as_bytes().first().is_some_and(u8::is_ascii_uppercase)
        && parts.iter().all(|p| {
            !p.is_empty()
                && p.bytes()
                    .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
        })
}

/// `hasDomainStemHead`.
fn has_domain_stem_head(name: &str) -> bool {
    let lower = name.to_lowercase();
    let domain = DOMAIN_STEMS.iter().any(|stem| {
        lower.starts_with(stem) && {
            let next = js_char_at(name, stem.len());
            next.is_none() || is_word_tail_boundary(next)
        }
    });
    domain
        || SUFFIX_REQUIRED_STEMS.iter().any(|stem| {
            lower.starts_with(stem) && is_word_tail_boundary(js_char_at(name, stem.len()))
        })
}

/// `/^[A-Za-z]{1,2}[0-9_]/`.
fn has_mint_head(name: &str) -> bool {
    let b = name.as_bytes();
    let tail = |c: Option<&u8>| c.is_some_and(|c| c.is_ascii_digit() || *c == b'_');
    match b.first() {
        Some(c) if c.is_ascii_alphabetic() => {
            tail(b.get(1)) || (b.get(1).is_some_and(u8::is_ascii_alphabetic) && tail(b.get(2)))
        }
        _ => false,
    }
}

/// `isBunToken`: the shape of a minifier-minted token — the WHO GETS
/// ASKED question. The census walk and the sweep's target collection read
/// it to decide what to process (single letters included since 2026-09-30,
/// Andrew: the old `SHORT_WORDS` exemption made never-asked letters
/// invisible to the sweep, finding #62); the carried rule and the vote
/// candidacy read it through [`is_below_floor_name`].
pub fn is_bun_token(name: &str) -> bool {
    if name.contains('$') || name.ends_with('_') {
        return true;
    }
    if is_constant_case(name) || has_domain_stem_head(name) {
        return false;
    }
    if has_mint_head(name) {
        return true;
    }
    js_len(name) <= 2 && !SHORT_WORDS.contains(&name.to_lowercase().as_str())
}

/// `isDecoratedDescriptive`: a descriptive stem wearing the conflict
/// ladder's trailing `_` (`fsPromises_`).
pub fn is_decorated_descriptive(name: &str) -> bool {
    if !name.ends_with('_') {
        return false;
    }
    let stem = name.trim_end_matches('_');
    !stem.is_empty() && !is_bun_token(stem)
}

/// `isBelowFloorName`: minted-shaped and not a decorated descriptive name.
/// Since 2026-09-30 this includes single letters — so a deliberately
/// APPLIED letter is recorded CARRIED (validated rename's exp066 rule) and
/// the sweep cannot re-roll it within the run.
pub fn is_below_floor_name(name: &str) -> bool {
    is_bun_token(name) && !is_decorated_descriptive(name)
}

/// A single alphabetic character (UTF-16 length 1) — the one mint shape
/// that can still be a deliberate NAME: a loop counter `i`, a coordinate
/// `x`, a catch parameter `e`. Andrew, 2026-09-30: producing a single
/// character output name is allowed "if necessary for a loop".
pub fn is_single_letter(name: &str) -> bool {
    js_len(name) == 1 && name.chars().next().is_some_and(char::is_alphabetic)
}

/// WHAT ANSWER MAY LAND: may the model's suggestion for a sweep target
/// become its new name? (`naming::passes::sweep`'s answer filter — see
/// the module doc for the target-vs-answer split.) Refuses re-minted
/// junk — `$`-bearing tokens, `_`-tails, `a1b`-shaped mint heads,
/// two-letter non-words — but accepts a single letter.
pub fn is_sweep_answer_acceptable(name: &str) -> bool {
    !is_bun_token(name) || is_single_letter(name)
}

/// Technical terms that carry a digit and look like a minifier stem but
/// are real vocabulary (`p2sBytes`, `x5cArray`, `LZ77Worker`,
/// `zodCidrV4` read as `CidRv4`) — the ones the 2026-10-03 replay over
/// 1.46M recorded answers found the model using legitimately, plus their
/// obvious siblings. Matched case-insensitively ANYWHERE in the answer —
/// a term that overlaps the candidate stem exempts it. The
/// [`DOMAIN_STEMS`] are NOT here: [`is_bun_token`] already keeps them out
/// of the stem set, and matching their short entries (`v8`, `es5`)
/// anywhere would exempt real borrowings (`initAv8`, `initializeS56`).
const TECH_TERMS: &[&str] = &[
    "p2c", "p2s", "b2c", "x5c", "x5t", "lz4", "lz77", "x10", "x11", "ie9", "ie10", "ie11", "md4",
    "cidrv4", "cidrv6", "sigv4", "es256", "es384", "es512", "rs256", "hs256", "ps256", "vp8",
    "vp8l", "vp9", "mp3", "mp4", "h264", "h265", "x86", "x64", "arm64",
];

/// The shape of a minified name another answer can BORROW as a word: a
/// minifier token ([`is_bun_token`]) of at least three UTF-16 units that
/// carries a digit — `H6t`, `uo7`, `D0u`, `A0n`, `da1`. Two-unit tokens
/// (`V2`, `y1`, `T4`) and digitless ones (`Etl`, `Ctl`) are left out on
/// purpose: in the replay they were mostly real words (`isV1Enabled`,
/// `y1Coordinate`), and precision comes first — a wrongly refused good
/// name forces a worse one.
pub fn is_borrowable_stem(name: &str) -> bool {
    js_len(name) >= 3 && name.bytes().any(|b| b.is_ascii_digit()) && is_bun_token(name)
}

/// The program's ORIGINAL minified binding names that have the
/// borrowable shape ([`is_borrowable_stem`]) — read once from the fresh
/// (pre-rename) text, so a neighbour renamed earlier in the run still
/// counts: the model saw its minified name in the code it was shown.
#[derive(Clone, Debug, Default)]
pub struct MinifiedStems(std::collections::HashSet<String>);

impl MinifiedStems {
    /// The borrowable names among `names` (every binding name of the
    /// input program).
    pub fn from_names<'n>(names: impl IntoIterator<Item = &'n str>) -> Self {
        MinifiedStems(
            names
                .into_iter()
                .filter(|n| is_borrowable_stem(n))
                .map(str::to_string)
                .collect(),
        )
    }

    /// Every symbol name of a parsed program (the fresh text's semantic).
    pub fn of_program(semantic: &oxc_semantic::Semantic<'_>) -> Self {
        let scoping = semantic.scoping();
        Self::from_names(scoping.symbol_ids().map(|s| scoping.symbol_name(s)))
    }

    pub fn contains(&self, name: &str) -> bool {
        self.0.contains(name)
    }
}

/// The word segments of a name, as byte ranges: split at `_` / `$`, and
/// before an ASCII capital that follows a lowercase letter or a digit
/// (`envVarD0u` → env, Var, D0u; `H6tClass` → H6t, Class).
fn word_segments(name: &str) -> Vec<(usize, usize)> {
    let b = name.as_bytes();
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    for (i, &c) in b.iter().enumerate() {
        if c == b'_' || c == b'$' {
            if let Some(s) = start.take() {
                out.push((s, i));
            }
            continue;
        }
        match start {
            None => start = Some(i),
            Some(s)
                if c.is_ascii_uppercase()
                    && (b[i - 1].is_ascii_lowercase() || b[i - 1].is_ascii_digit()) =>
            {
                out.push((s, i));
                start = Some(i);
            }
            Some(_) => {}
        }
    }
    if let Some(s) = start {
        out.push((s, b.len()));
    }
    out
}

/// Whether a [`TECH_TERMS`] entry overlaps the byte range `a..b`.
fn tech_term_covers(name: &str, a: usize, b: usize) -> bool {
    let lower = name.to_ascii_lowercase();
    TECH_TERMS.iter().any(|t| {
        lower
            .match_indices(t)
            .any(|(p, _)| p < b && a < p + t.len())
    })
}

/// WHAT ANSWER MAY LAND, the borrowed-stem half — the ONE answer-quality
/// question every LLM naming site asks (the wave barrier for function,
/// module and shadowed lanes; the coverage sweep): does the model's
/// answer wear one of the program's ORIGINAL minified names as a word
/// (`H6tClass`, `uo7Instance`, `envVarD0u`, `setMethodH6t`)? Returns the
/// borrowed stem. Such an answer is refused like an invalid one — a
/// disclosed re-ask naming the stem, within the `--rename-retries`
/// budget, then the binding stays unrenamed and EXHAUSTED; never applied,
/// never decorated.
///
/// Precision first (the 2026-10-03 replay, docs/rust-port/16-findings-
/// queue.md): the stem must be a word segment, never the whole answer;
/// it must have the borrowable shape ([`is_borrowable_stem`]) AND be a
/// binding name of this program; a CONSTANT_CASE answer and a segment a
/// technical term covers (`p2sBytes`, `b2cLoginHosts`, `x5cArray`) are
/// exempt. `is2017OrLater` / `sha256Hash` pass because `is2017` and
/// `sha256` are not minified bindings.
pub fn borrowed_minified_stem<'a>(answer: &'a str, stems: &MinifiedStems) -> Option<&'a str> {
    if is_constant_case(answer) {
        return None;
    }
    word_segments(answer).into_iter().find_map(|(a, b)| {
        let segment = &answer[a..b];
        let borrowed = (a, b) != (0, answer.len())
            && stems.contains(segment)
            && is_borrowable_stem(segment)
            && !tech_term_covers(answer, a, b);
        borrowed.then_some(segment)
    })
}

/// A deliberate convention placeholder: all-underscore (`_`, `__`) or
/// `$`-only. The ONE name-shape exception that survives in the coverage
/// sweep's target path under the 2026-09-30 provenance decision — these
/// are intentional discards (a `_` catch, a minifier's `$` temp), never
/// "not properly renamed", so asking the model about them would only
/// mint noise. The list is deliberately this short; every entry is a
/// convention, not a heuristic.
pub fn is_convention_carveout(name: &str) -> bool {
    !name.is_empty() && name.bytes().all(|b| b == b'_' || b == b'$')
}

/// `isWordlessMintShape`: no 3-letter lowercase word run and not a
/// CONSTANT_CASE constant — the reconcile's coarse mint metric (flags
/// `iIn`, which `is_bun_token` cannot see; does NOT flag the half-mint
/// `do7Function`, which has a word run).
pub fn is_wordless_mint_shape(name: &str) -> bool {
    if is_constant_case(name) {
        return false;
    }
    !name
        .as_bytes()
        .windows(3)
        .any(|w| w.iter().all(u8::is_ascii_lowercase))
}

/// `isHalfMintHead`: a short mint stem wearing a capitalized word tail
/// (`do7Function`, `T7Class`, `sm6Factory`, `h06Result`, `j3lResult`) —
/// `/^(?:[A-Za-z][0-9]{1,2}|[A-Za-z]{2}[0-9]|[A-Za-z][0-9][a-z])[A-Z][a-z]/`
/// behind the `isBunToken` gate.
pub fn is_half_mint_head(name: &str) -> bool {
    if !is_bun_token(name) {
        return false;
    }
    let b = name.as_bytes();
    let alpha = |i: usize| b.get(i).is_some_and(u8::is_ascii_alphabetic);
    let digit = |i: usize| b.get(i).is_some_and(u8::is_ascii_digit);
    let lower = |i: usize| b.get(i).is_some_and(u8::is_ascii_lowercase);
    let upper = |i: usize| b.get(i).is_some_and(u8::is_ascii_uppercase);
    let tail_at = |i: usize| upper(i) && lower(i + 1);
    if !alpha(0) {
        return false;
    }
    let heads = [
        digit(1),             // [A-Za-z][0-9]
        digit(1) && digit(2), // [A-Za-z][0-9]{2}
        alpha(1) && digit(2), // [A-Za-z]{2}[0-9]
        digit(1) && lower(2), // [A-Za-z][0-9][a-z]
    ];
    let lens = [2usize, 3, 3, 3];
    heads.iter().zip(lens).any(|(&ok, len)| ok && tail_at(len))
}

#[cfg(test)]
mod floor_test;
