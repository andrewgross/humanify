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
//!   placeholders, not un-renamed code). [`is_minifier_token`] still
//!   answers the shape question where shape IS the question: the
//!   minted-census METER's population, the class-id floor's derivation
//!   filter, vote candidacy, and the below-floor/carried rules.
//! - WHAT ANSWER MAY LAND — may the model's suggestion become the new
//!   name? ([`is_sweep_answer_acceptable`]: the sweep's answer filter.)
//!   This stays a STRING question by nature ("is this candidate junk?"),
//!   unchanged by the provenance decision: a single letter passes, whole
//!   minifier-shaped tokens (`a1b`, `x_`, `Kq$`) stay refused.
//!   Since 2026-10-03 its BORROWED-STEM half ([`borrowed_minified_stem`]:
//!   `H6tClass` wears the program's minified `H6t`) is asked by EVERY
//!   LLM naming site — the wave barrier and the sweep alike. The shape
//!   half stays the sweep's alone, a declared difference: replayed over
//!   the main pass's recorded answers it would refuse `iv`, `fd`, `x1`
//!   (461 answers under the old Bun shapes, mostly real names; the long
//!   ones — `is2017OrLater`, `LZ77Compressor` — pass since 2026-10-06).
//!   Round 2 (2026-10-03) added the IDENTITY-ECHO half
//!   ([`is_minified_echo`]: `{"yl":"yl"}`), asked by the same sites
//!   through `naming::waves::processor::answer_refusal`.
//!
//! WHICH MINIFIER's shapes (2026-10-03, Andrew: "loaded dynamically based
//! on the detected minifier ... the generic ones are fine to run always"):
//! every shape predicate takes the run's [`NameProfile`], selected once
//! from detection by `rename::name_profile::select_name_profile`. Since
//! 2026-10-06 (scan B1) every minifier profile, Bun's included, reads the
//! one measured renamer shape; the Bun profile's old Claude-Code-calibrated
//! long shapes are gone, and "the answer copies a minified name" is the
//! program lookup's question ([`borrowed_minified_stem`]). What each
//! profile reads, and which checks stay profile-independent, is that
//! module's doc.
//! Profile-independent here: [`is_single_letter`],
//! [`is_convention_carveout`], [`is_wordless_mint_shape`], and the
//! program lookup half of [`borrowed_minified_stem`].

use crate::rename::name_profile::NameProfile;

/// Short words that are real names, not mints (`SHORT_WORDS`). An OPEN
/// list of general programming vocabulary, collected from false positives
/// seen on Claude Code runs (the only app measured) — not app vocabulary;
/// add a general word when another app trips on it. The ten
/// single letters a/b/e/i/j/k/n/t/x/y left this list 2026-09-30 (Andrew:
/// "remove the limit on single character names not being processed" —
/// every profile treats them as minted now); the real two/three-letter
/// words stay.
const SHORT_WORDS: &[&str] = &[
    "abs", "add", "arg", "cb", "col", "ctx", "cwd", "db", "del", "dir", "end", "env", "err", "ext",
    "fn", "fs", "get", "gid", "go", "has", "id", "idx", "io", "ip", "key", "len", "log", "map",
    "max", "min", "msg", "now", "num", "obj", "ok", "os", "out", "pid", "pos", "raw", "req", "res",
    "row", "run", "sep", "set", "str", "sum", "tag", "ui", "uid", "url", "val",
];

/// Domain stems a mint-shaped head is allowed to carry (`DOMAIN_STEMS`).
/// An OPEN list of general programming vocabulary (encodings, hashes,
/// protocols, cloud/standards names), collected from false positives in
/// Claude Code census dumps (`k8s`, `b64`, `u2f`, `x509` from 2.1.216) —
/// not app vocabulary; other apps will need more (`ed25519`, `p256`, …).
const DOMAIN_STEMS: &[&str] = &[
    "e164", "ec2", "s3", "sha1", "sha256", "sha512", "md5", "utf8", "utf16", "base64", "http2",
    "oauth2", "i18n", "l10n", "a11y", "es5", "es6", "es2015", "ipv4", "ipv6", "v8", "w3c", "k8s",
    "b64", "u2f", "x509",
];

/// Stems that count only with a word tail (`SUFFIX_REQUIRED_STEMS`).
/// General vocabulary only: `it2` (iTerm2's CLI, Claude Code's own
/// vocabulary from its 2.1.216 census) was removed 2026-10-06 — no app's
/// words in a pipeline rule, and under the shared renamer shape the
/// model's `it2Command` is not mint-shaped anyway.
const SUFFIX_REQUIRED_STEMS: &[&str] = &["h1", "h2", "h3", "h4", "h5", "h6", "v1", "x0"];

/// UTF-16 length (JS `name.length`).
fn js_len(name: &str) -> usize {
    name.encode_utf16().count()
}

/// `isWordTailBoundary`: the unit at `index` starts a word tail — an
/// ASCII capital, or a `_` that something follows (since 2026-10-06: a
/// bare trailing `_` is the conflict ladder's, or Bun's own `v8_`/`s3_`
/// mint, not the start of `v8_engine`).
fn is_word_tail_boundary(name: &str, index: usize) -> bool {
    let units: Vec<u16> = name.encode_utf16().skip(index).take(2).collect();
    match units.first() {
        Some(&c) if c == u16::from(b'_') => units.len() > 1,
        Some(&c) => (u16::from(b'A')..=u16::from(b'Z')).contains(&c),
        None => false,
    }
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
        lower.starts_with(stem)
            && (js_len(name) == stem.len() || is_word_tail_boundary(name, stem.len()))
    });
    domain
        || SUFFIX_REQUIRED_STEMS
            .iter()
            .any(|stem| lower.starts_with(stem) && is_word_tail_boundary(name, stem.len()))
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

/// The shape of a minifier-minted token under the run's
/// [`NameProfile`] — the WHO GETS ASKED question. The census walk and the
/// sweep's target collection read it to decide what to process (single
/// letters included since 2026-09-30, Andrew: the old `SHORT_WORDS`
/// exemption made never-asked letters invisible to the sweep, finding
/// #62); the carried rule and the vote candidacy read it through
/// [`is_below_floor_name`]. What each profile reads:
/// `rename::name_profile`'s module doc.
pub fn is_minifier_token(profile: NameProfile, name: &str) -> bool {
    match profile {
        NameProfile::Bun | NameProfile::Esbuild | NameProfile::Terser | NameProfile::Swc => {
            is_renamer_token(name)
        }
        NameProfile::NotMinified => false,
    }
}

/// The measured renamer shape every minifier profile reads (Bun's
/// included since 2026-10-06, scan B1): a 1-2 unit non-word, or a WHOLE
/// 3-4 unit name in the renamer alphabet carrying a mint head or a `$`
/// (`p5e`, `ab_`, `id_`, `$me`) — behind the exemptions. Otherwise a
/// trailing `_` is judged by the name it decorates: `_k_`, `H2_` are
/// minted (Bun emits them), `fsPromises_` is a real name wearing the
/// conflict ladder's tail.
///
/// Bun's profile used to add three rules calibrated on the Claude Code
/// corpus — `$` anywhere, any trailing `_`, a mint head at ANY length —
/// to catch the model's half-copied answers (`do7Function`). They
/// misfired on real names in other apps (RxJS `user$`, Angular `$scope`,
/// Svelte `$store`, snake_case `to_string`), and what they were for is
/// the program lookup's job ([`borrowed_minified_stem`]: the answer wears
/// one of THIS program's minified names), which works for any app.
fn is_renamer_token(name: &str) -> bool {
    let whole = !is_exempt_shape(name)
        && (is_short_non_word(name)
            || ((3..=4).contains(&js_len(name))
                && is_renamer_alphabet(name)
                && (has_mint_head(name) || name.contains('$'))));
    let stem = name.trim_end_matches('_');
    whole || (!stem.is_empty() && stem.len() < name.len() && is_renamer_token(stem))
}

/// `[A-Za-z_$][A-Za-z0-9_$]*` — every measured renamer's alphabet (a
/// digit is never first).
fn is_renamer_alphabet(name: &str) -> bool {
    let word = |b: &u8| b.is_ascii_alphanumeric() || *b == b'_' || *b == b'$';
    let b = name.as_bytes();
    b.first().is_some_and(|c| !c.is_ascii_digit()) && b.iter().all(word)
}

/// The precision exemptions every profile shares: a CONSTANT_CASE
/// constant or a domain-stem head (`sha256`, `v8Engine`, `h1Title`).
fn is_exempt_shape(name: &str) -> bool {
    is_constant_case(name) || has_domain_stem_head(name)
}

/// At most two UTF-16 units and not a real short word ([`SHORT_WORDS`]).
fn is_short_non_word(name: &str) -> bool {
    js_len(name) <= 2 && !SHORT_WORDS.contains(&name.to_lowercase().as_str())
}

/// `isDecoratedDescriptive`: a descriptive stem wearing the conflict
/// ladder's trailing `_` (`fsPromises_`).
pub fn is_decorated_descriptive(profile: NameProfile, name: &str) -> bool {
    if !name.ends_with('_') {
        return false;
    }
    let stem = name.trim_end_matches('_');
    !stem.is_empty() && !is_minifier_token(profile, stem)
}

/// `isBelowFloorName`: minted-shaped and not a decorated descriptive name.
/// Since 2026-09-30 this includes single letters — so a deliberately
/// APPLIED letter is recorded CARRIED (validated rename's exp066 rule) and
/// the sweep cannot re-roll it within the run.
pub fn is_below_floor_name(profile: NameProfile, name: &str) -> bool {
    is_minifier_token(profile, name) && !is_decorated_descriptive(profile, name)
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
/// The junk shapes are the run's [`NameProfile`]'s: only a whole
/// minifier-shaped token refuses (under Bun too since 2026-10-06 — a
/// longer answer copying a minified name is refused by the program lookup,
/// [`borrowed_minified_stem`], at every naming site).
pub fn is_sweep_answer_acceptable(profile: NameProfile, name: &str) -> bool {
    !is_minifier_token(profile, name) || is_single_letter(name)
}

/// Technical terms that carry a digit and look like a minifier stem but
/// are real vocabulary (`p2sBytes`, `x5cArray`, `LZ77Worker`,
/// `zodCidrV4` read as `CidRv4`) — the ones the 2026-10-03 replay over
/// 1.46M recorded answers found the model using legitimately, plus their
/// obvious siblings. Those answers were all from Claude Code runs; the
/// terms are general (JOSE, compression, codecs, CPU arches, networking),
/// and the list is OPEN. Matched case-insensitively ANYWHERE in the answer —
/// a term that overlaps the candidate stem exempts it. The
/// [`DOMAIN_STEMS`] are NOT here: [`is_minifier_token`] already keeps them
/// out of the stem set, and matching their short entries (`v8`, `es5`)
/// anywhere would exempt real borrowings (`initAv8`, `initializeS56`).
const TECH_TERMS: &[&str] = &[
    "p2c", "p2s", "b2c", "x5c", "x5t", "lz4", "lz77", "x10", "x11", "ie9", "ie10", "ie11", "md4",
    "cidrv4", "cidrv6", "sigv4", "es256", "es384", "es512", "rs256", "hs256", "ps256", "vp8",
    "vp8l", "vp9", "mp3", "mp4", "h264", "h265", "x86", "x64", "arm64",
    // Round 2 (2026-10-03): `isE2E` (end-to-end) beside a program `e2e`.
    "e2e",
];

/// The shape of a minified name another answer can BORROW as a word: a
/// minifier token of at least three UTF-16 units that carries a digit —
/// `H6t`, `uo7`, `D0u`, `A0n`, `da1`. Two-unit tokens (`V2`, `y1`, `T4`)
/// and digitless ones (`Etl`, `Ctl`) are left out on purpose: in the
/// replay they were mostly real words (`isV1Enabled`, `y1Coordinate`),
/// and precision comes first — a wrongly refused good name forces a
/// worse one.
///
/// This is the PRECISION GUARD of an always-on check, not its evidence
/// (the evidence is the program lookup, [`MinifiedStems`]). Under every
/// profile — NotMinified included — it is the renamer-alphabet token (a
/// whole 3-4 unit name with a mint head, `p5e`): NotMinified's own token
/// reads nothing, which would switch an always-on check off, and the
/// renamer alphabet is the one shape every measured minifier emits for its
/// 3-character names (`rename::name_profile`). On an unminified program
/// such names are rare, and the lookup still has to find one bound. The
/// profile argument is kept so a measured per-minifier difference has
/// somewhere to land.
///
/// A name carrying a `$` or `_` (since 2026-10-06, scan B1 — the Bun
/// profile's `$`/`_` shape rules used to catch these copies) is borrowable
/// from two units when it also carries a digit or a capital (`Jw$`,
/// `Uw_`, `$Iq`, `A$`, `$6`): no word is spelled so. All-lowercase ones
/// (`$el`, `is_`, `w$i`) are left out — precision first — and so are
/// the convention placeholders (`$`, `_`, `$$`).
pub fn is_borrowable_stem(_profile: NameProfile, name: &str) -> bool {
    let has = |f: fn(&u8) -> bool| name.bytes().any(|b| f(&b));
    let shaped = is_renamer_token(name) && !is_convention_carveout(name);
    if has(|b| *b == b'$' || *b == b'_') {
        js_len(name) >= 2 && shaped && has(|b| b.is_ascii_digit() || b.is_ascii_uppercase())
    } else {
        js_len(name) >= 3 && has(u8::is_ascii_digit) && shaped
    }
}

/// The program's ORIGINAL minified binding names that have the
/// borrowable shape ([`is_borrowable_stem`]) — read once from the fresh
/// (pre-rename) text, so a neighbour renamed earlier in the run still
/// counts: the model saw its minified name in the code it was shown.
/// Membership is CASE-INSENSITIVE (round 2, 2026-10-03 — the model
/// re-cases what it borrows: `k2hResult` for the program's `K2H`), with
/// the exact spellings kept for the one place that needs them (a trailing
/// numbered word, see [`borrowed_minified_stem`]). It carries the run's
/// [`NameProfile`], so every LLM naming site that holds the stems asks
/// the answer checks under the same profile.
#[derive(Clone, Debug)]
pub struct MinifiedStems {
    profile: NameProfile,
    exact: std::collections::HashSet<String>,
    folded: std::collections::HashSet<String>,
}

impl MinifiedStems {
    /// The borrowable names among `names` (every binding name of the
    /// input program), under `profile`.
    pub fn from_names<'n>(profile: NameProfile, names: impl IntoIterator<Item = &'n str>) -> Self {
        let exact: std::collections::HashSet<String> = names
            .into_iter()
            .filter(|n| is_borrowable_stem(profile, n))
            .map(str::to_string)
            .collect();
        let folded = exact.iter().map(|n| n.to_ascii_lowercase()).collect();
        MinifiedStems {
            profile,
            exact,
            folded,
        }
    }

    /// No stems at all (a pass that runs without the fresh program).
    pub fn empty(profile: NameProfile) -> Self {
        Self::from_names(profile, [])
    }

    /// Every symbol name of a parsed program (the fresh text's semantic).
    pub fn of_program(profile: NameProfile, semantic: &oxc_semantic::Semantic<'_>) -> Self {
        let scoping = semantic.scoping();
        Self::from_names(
            profile,
            scoping.symbol_ids().map(|s| scoping.symbol_name(s)),
        )
    }

    /// The name profile the stems (and every answer check beside them)
    /// were selected under.
    pub fn profile(&self) -> NameProfile {
        self.profile
    }

    /// Whether the program binds `name` in any ASCII casing.
    pub fn contains(&self, name: &str) -> bool {
        self.folded.contains(&name.to_ascii_lowercase())
    }

    /// Whether the program binds `name` spelled exactly so.
    pub fn contains_exact(&self, name: &str) -> bool {
        self.exact.contains(name)
    }
}

/// A capitalised word followed only by digits (`Fn3`, `Go4`) — at the END
/// of an answer this reads as a numbered variant (`emptyFn3` beside
/// `emptyFn2`), so it counts as borrowed only on an EXACT-case match.
fn is_numbered_word(segment: &str) -> bool {
    let b = segment.as_bytes();
    let letters = b.iter().take_while(|c| c.is_ascii_alphabetic()).count();
    letters >= 2
        && letters < b.len()
        && b[0].is_ascii_uppercase()
        && b[1..letters].iter().all(u8::is_ascii_lowercase)
        && b[letters..].iter().all(u8::is_ascii_digit)
}

/// The stem CANDIDATES of a name, as byte ranges: every word segment
/// ([`word_segments`]), plus — when a segment ends in a digit and the
/// next starts a run of capitals — the segment continued through that
/// run up to the capital that starts the next word (round 2, 2026-10-03:
/// `S2KFunction` segments as `S2` + `KFunction`, and the program's `S2K`
/// is the candidate `S2` + `K`; `Q2KXHandler` → `Q2KX`; `valueS2K` →
/// `S2K`). The continued candidate comes first: it is the longer stem.
fn stem_candidates(name: &str) -> Vec<(usize, usize)> {
    let b = name.as_bytes();
    let mut out = Vec::new();
    for (a, e) in word_segments(name) {
        if e > a && b[e - 1].is_ascii_digit() && b.get(e).is_some_and(u8::is_ascii_uppercase) {
            let mut j = e;
            while b.get(j).is_some_and(u8::is_ascii_uppercase) {
                j += 1;
            }
            // The last capital of a run that a lowercase letter follows
            // starts the next word (`KFunction`: `F` begins `Function`).
            if b.get(j).is_some_and(u8::is_ascii_lowercase) {
                j -= 1;
            }
            if j > e {
                out.push((a, j));
            }
        }
        out.push((a, e));
    }
    out
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
///
/// Round 2 (2026-10-03): the comparison is CASE-INSENSITIVE
/// ([`MinifiedStems`]; `go4Function` borrows `Go4`), and the candidates
/// include a digit-ended segment continued through the capitals after it
/// ([`stem_candidates`]; `S2KFunction` borrows `S2K`). The returned stem
/// is the ANSWER's spelling — the word the re-ask tells the model to drop.
/// One case exception, from the round-2 replay: a capitalised word plus
/// digits at the very END (`emptyFn3`, `noopFn3` — numbered variants; the
/// program binds `fn3`) needs an EXACT-case match, as before round 2. And
/// a stem whose digits run on as `_<digit>` is a version number
/// (`migrateSonnet1mTo4_5` — the one legitimate name main's predicate
/// refused in the round-2 replay).
///
/// 2026-10-06 (scan B1): a `$`/`_`-bearing piece of the answer is tested
/// too ([`symbol_stem_candidates`]; `initJw$` borrows `Jw$`, `assignUw_`
/// `Uw_`, `initModule_$Iq` `$Iq`) — the copies the Bun profile's
/// `$`-anywhere and `_`-tail shape rules used to refuse, now caught by
/// THIS program's names instead of a shape, so it works for any app.
pub fn borrowed_minified_stem<'a>(answer: &'a str, stems: &MinifiedStems) -> Option<&'a str> {
    if is_constant_case(answer) {
        return None;
    }
    borrowed_word_stem(answer, stems).or_else(|| borrowed_symbol_stem(answer, stems))
}

/// The `$`/`_`-bearing half of [`borrowed_minified_stem`]: the program
/// binds the piece spelled EXACTLY so (the program's spelling passed
/// [`is_borrowable_stem`] on the way into [`MinifiedStems`], so the piece
/// carries a digit or a capital too). Exact case on purpose: case-folded,
/// the replay refused `$jQuery` (the program binds `$J`).
fn borrowed_symbol_stem<'a>(answer: &'a str, stems: &MinifiedStems) -> Option<&'a str> {
    symbol_stem_candidates(answer)
        .into_iter()
        .map(|(a, b)| &answer[a..b])
        .find(|piece| stems.contains_exact(piece))
}

/// The `$`/`_`-bearing pieces of an ASCII answer (2-4 units, never the
/// whole answer) that start at a word segment or a `$`/`_` and end at a
/// word segment's end or just after a `$`/`_` (`initJw$` → `Jw$`;
/// `n$_Result` → `n$`, `n$_`; `initModule_$Iq` → `$Iq`, `_$Iq`). A piece
/// whose only symbols are `_` counts only where its `_` begins or ends
/// the answer (`assignUw_` → `Uw_`): an inner `_` is a snake_case
/// separator, so `is_valid` never lends `is_`.
fn symbol_stem_candidates(answer: &str) -> Vec<(usize, usize)> {
    let b = answer.as_bytes();
    if !answer.is_ascii() {
        return Vec::new();
    }
    let sym = |c: u8| c == b'$' || c == b'_';
    let segments = word_segments(answer);
    let is_start = |i: usize| sym(b[i]) || segments.iter().any(|&(s, _)| s == i);
    let is_end = |i: usize| sym(b[i - 1]) || segments.iter().any(|&(_, e)| e == i);
    let underscore_ok = |s: usize, t: usize| {
        let mut inner = &answer[s..t];
        if s == 0 {
            inner = inner.trim_start_matches('_');
        }
        if t == answer.len() {
            inner = inner.trim_end_matches('_');
        }
        answer[s..t].contains('$') || !inner.contains('_')
    };
    let mut out = Vec::new();
    for s in (0..b.len()).filter(|&i| is_start(i)) {
        for t in (s + 2..=(s + 4).min(b.len())).filter(|&t| is_end(t)) {
            if (s, t) != (0, b.len()) && b[s..t].iter().any(|c| sym(*c)) && underscore_ok(s, t) {
                out.push((s, t));
            }
        }
    }
    out
}

/// The word half of [`borrowed_minified_stem`] (the 2026-10-03 rules).
fn borrowed_word_stem<'a>(answer: &'a str, stems: &MinifiedStems) -> Option<&'a str> {
    stem_candidates(answer).into_iter().find_map(|(a, b)| {
        let segment = &answer[a..b];
        let bound = if b == answer.len() && is_numbered_word(segment) {
            stems.contains_exact(segment)
        } else {
            stems.contains(segment)
        };
        // Digits continued by `_<digit>` spell a version (`To4_5` = "to
        // 4.5" in `migrateSonnet1mTo4_5`), not a minified name.
        let version = answer.as_bytes()[b..].starts_with(b"_")
            && answer.as_bytes().get(b + 1).is_some_and(u8::is_ascii_digit);
        let borrowed = (a, b) != (0, answer.len())
            && bound
            && !version
            && is_borrowable_stem(stems.profile, segment)
            && !tech_term_covers(answer, a, b);
        borrowed.then_some(segment)
    })
}

/// WHAT ANSWER MAY LAND, the identity-echo half (round 2, 2026-10-03):
/// is `answer` the asked identifier's own MINIFIED name handed back
/// (`{"yl":"yl","zf":"zf"}`)? True when the answer equals the name and
/// the name is a minifier token ([`is_minifier_token`], under the run's
/// profile) of two or more UTF-16 units that is not a convention
/// placeholder. Single letters are left
/// out on purpose (Andrew, 2026-09-30: a loop counter `i` may stay), and
/// so are real short words (`fs`, `id` are not tokens) and every
/// descriptive name. Every LLM naming site refuses such an echo like a
/// borrowed answer: a disclosed re-ask ("`yl` is the minified name"),
/// within `--rename-retries`, then unrenamed and EXHAUSTED — a sweep
/// target, never a decided keep. Three-letter digitless mints (`RHe`,
/// `Etl`) are minifier tokens under no profile and keep their echo
/// (precision first: the shape cannot tell them from words).
pub fn is_minified_echo(profile: NameProfile, old: &str, answer: &str) -> bool {
    answer == old
        && js_len(old) >= 2
        && is_minifier_token(profile, old)
        && !is_convention_carveout(old)
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
/// `iIn`, which no profile's token reads; does NOT flag the half-mint
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
/// behind the profile's gate. Every profile's token is a whole short name,
/// so the gate is the HEAD's: the stem before the word tail must itself be
/// a minifier token (`do7`, `T7`; nothing under NotMinified), and the
/// whole name must not be an exempt shape (`h1Title`, `v8Engine`).
pub fn is_half_mint_head(profile: NameProfile, name: &str) -> bool {
    let gate =
        |head_len: usize| !is_exempt_shape(name) && is_minifier_token(profile, &name[..head_len]);
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
    heads
        .iter()
        .zip(lens)
        .any(|(&ok, len)| ok && tail_at(len) && gate(len))
}

#[cfg(test)]
mod floor_test;
