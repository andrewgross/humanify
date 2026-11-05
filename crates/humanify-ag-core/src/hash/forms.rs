//! The hash FORMS — every "is this the same code?" canonical form, as a
//! NAMED, VERSIONED setting (docs/matching-architecture.md §3.2, step 2).
//!
//! A form says which differences between two pieces of code its digest
//! ignores: local names, string contents, numbers, property names, class
//! private names, free (global) names, the arrow/function wrapper
//! spelling. Every caller names the form it hashes under; nothing passes
//! a loose flag. Each form's VERSION lives here, beside its settings, and
//! [`FORMS`] lists them all — the one answer to "what hash forms did this
//! run use?".
//!
//! THREE WALKS produce the forms, and they stay three on purpose:
//!
//! - [`Walk::Canonical`] — `hash::serialize`, oxc's ESTree in serde's
//!   alphabetical key order, every scalar in the stream, locals numbered by
//!   first occurrence (the slot ORDINALS are what lines prior names up with
//!   fresh ones). Most forms are settings of this walk.
//! - [`Walk::Statement`] — `hash::statement_hash`, the split's statement
//!   identity: node types + a per-type content string only, EVERY name
//!   masked with no numbering (property names included: the split must
//!   match code across humanify's own export-member renames), iterative so
//!   a bundle-sized statement cannot overflow the stack. It cannot be a
//!   setting of the canonical walk without changing bytes: the canonical
//!   stream carries every scalar and numbers its slots; this one carries
//!   neither, and its digest is persisted (the split ledger).
//! - [`Walk::AlignTokens`] — `matching::statement_align`'s token walk, a
//!   token STREAM (k-gram shingles need token boundaries) in BABEL's parsed
//!   field order with babel's node names (`ObjectProperty` for oxc's
//!   `Property`, optional chains unwrapped). Its slot ordinals decide the
//!   close tier's transfer order and its k-grams depend on where a change
//!   lands in the stream, so its order cannot be the canonical walk's.
//!   It shares the canonical walk's rules (identifier roles, literal
//!   classes, block unwrap, escaping) from `hash::serialize`.
//!
//! A deliberate behaviour change to a form is a NEW VERSION of it (and an
//! eval), never an edit under the old number: a persisted form's readers
//! refuse a mismatched stamp, and that refusal is the only thing standing
//! between an old digest and a silent half-join (exp093's lesson).
//! `hash_forms_pin_test` pins every form's bytes over the e2e fixtures.

/// Which walk produces the form's stream (module doc: why three).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Walk {
    Canonical,
    Statement,
    AlignTokens,
}

/// Local (scope-bound) names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Locals {
    /// A numbered slot per binding, `$0, $1, …` by first occurrence.
    Slots,
    /// Slotted, then every ordinal blinded to `$` / `L` (an inserted
    /// declaration cannot renumber unchanged content's shingles).
    SlotsUnnumbered,
    /// Masked to the node type, no slot at all.
    Masked,
}

/// String and template-text literals.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Strings {
    Exact,
    /// The volatile classes (semver, ISO-8601, hex digest) or a length
    /// marker (`__STR_<len>__`).
    Blurred,
}

/// Number and bigint literals.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Numbers {
    /// Exact under every identity form since exp093.
    Exact,
    /// Reduced to the class marker (`N` / `B`).
    Masked,
}

/// Static property / member / key names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PropertyNames {
    Verbatim,
    Masked,
}

/// Class-private names (`#x`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Privates {
    Verbatim,
    /// Every private token reads `P=#` (blinded at token emission).
    Blinded,
    /// Masked with every other name (the statement walk).
    Masked,
}

/// Free (unresolved, global) names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FreeNames {
    Verbatim,
    /// Slotted unless a real global the code's own program does not
    /// declare (`modules::known_globals`): references to the rest of a
    /// bundle, whose spellings are release-specific. Realized by the
    /// CALLER (`modules::vendor_content` declares those names around the
    /// factory and re-parses, so the canonical walk slots them).
    SlottedUnlessGlobal,
    Masked,
}

/// Which object-member node names the walk's parents carry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParentNames {
    /// oxc's ESTree: every object member is `Property`.
    Oxc,
    /// babel's: `ObjectProperty` / `ObjectMethod`.
    Babel,
}

/// One named, versioned canonical form.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CanonicalForm {
    /// The form's name (`<name>/<version>` is how a stamp spells it).
    pub name: &'static str,
    /// The form's era. Persisted forms are refused across a mismatch;
    /// in-run forms (both sides hashed by the same binary) still carry
    /// one, so a change to them is just as visible.
    pub version: u64,
    /// Where the version is written to disk, when it is (None = both
    /// sides of every comparison are hashed in the same run).
    pub stamped_in: Option<&'static str>,
    pub walk: Walk,
    pub locals: Locals,
    pub strings: Strings,
    pub numbers: Numbers,
    pub property_names: PropertyNames,
    pub privates: Privates,
    pub free_names: FreeNames,
    /// exp094: a SAFE arrow walks under the `FunctionExpression` token
    /// (`hash::wrapper_spelling`, the one rule).
    pub wrapper_spelling: bool,
    pub parent_names: ParentNames,
}

// ---------------------------------------------------------------------------
// The canonical walk's forms
// ---------------------------------------------------------------------------

/// The cross-version match key: function fingerprints (`graph`), factory
/// structural hashes (`modules::factory_structural_hash`), the enclosing-
/// statement contexts (`matching::statement_context`), the twin gates'
/// first comparison (`twins::gates`). Version = the factory records' era,
/// the vendor manifest's `hashVersion` (`unpack::bun`): a manifest stamped
/// with ANY other version — the TS (no stamp; bytes are
/// `bun-module-classification.ts`'s) or an older Rust era — never joins
/// this run's hashes; the carry-over re-keys it by CONTENT
/// (`modules::vendor_content`, [`VENDOR_CONTENT`]).
///
/// 2 = `factory_structural_hash` at WP5.6e (2026-09-25).
/// 3 = exp093 (2026-09-29): numbers EXACT — old buckets' hashes are a
///     different era's bytes, re-derived, never silently matched (a
///     number-free factory's bytes coincide across the era, so the version
///     gate alone stands between a stale manifest and a half-carried
///     prior).
/// 4 = exp094 (2026-10-02): a SAFE arrow serializes under the
///     FunctionExpression token (the wrapper-spelling unification), so
///     arrow-factory hashes are a new era's bytes. A factory spelled as a
///     function expression keeps its bytes ACROSS the era — the same
///     half-carried-prior shape exp093's gate exists for — so the version
///     gate alone again stands between a stale manifest and a silent join.
pub const MATCH_KEY: CanonicalForm = CanonicalForm {
    name: "match-key",
    version: 4,
    stamped_in: Some("vendor manifest hashVersion (factory structuralHash)"),
    walk: Walk::Canonical,
    locals: Locals::Slots,
    strings: Strings::Blurred,
    numbers: Numbers::Exact,
    property_names: PropertyNames::Verbatim,
    privates: Privates::Verbatim,
    free_names: FreeNames::Verbatim,
    wrapper_spelling: true,
    parent_names: ParentNames::Oxc,
};

/// The match key with private names blinded: the twin gates' masked
/// fallback (a private re-lettering reconciles; TS
/// `privateMaskedStreamsEqual`). Moves with [`MATCH_KEY`].
pub const MATCH_KEY_PRIVATES_BLINDED: CanonicalForm = CanonicalForm {
    name: "match-key-privates-blinded",
    stamped_in: None,
    privates: Privates::Blinded,
    ..MATCH_KEY
};

/// The library index's statement-unit prints (`libindex::units`): the
/// match key with private names blinded (a minifier's `#e` is renamed per
/// build). Its own stamp — the index manifest's `units` — because an index
/// is built offline and read by a later binary. The version also covers
/// the UNIT MODEL (what a unit is: `graph::references::container_units`),
/// pinned by `libindex::units_test`'s `UNIT_MODEL_PINS`: 3 = esbuild 0.28's
/// guarded `__esm` is a lazy module (main's 68b090e3; exp097).
pub const LIBRARY_UNITS: CanonicalForm = CanonicalForm {
    name: "units",
    version: 3,
    stamped_in: Some("library index manifest `units`"),
    privates: Privates::Blinded,
    ..MATCH_KEY
};

/// The vendor content key (`modules::vendor_content`): a factory's match
/// key with every free name that is not a real global slotted — the
/// cross-release module key, and the library index's whole-module print.
/// Moves with [`MATCH_KEY`] (its walk IS the match key's).
pub const VENDOR_CONTENT: CanonicalForm = CanonicalForm {
    name: "vendor-content-key",
    stamped_in: Some("library index manifest `module`"),
    free_names: FreeNames::SlottedUnlessGlobal,
    ..MATCH_KEY
};

/// The same-release identity key: strings exact, no wrapper-spelling rule
/// (byte-exact identity). Module-binding fingerprints (`graph`), the whole-
/// file inherit signature (`finish::vendor_inherit`).
pub const IDENTITY_KEY: CanonicalForm = CanonicalForm {
    name: "identity-key",
    version: 1,
    stamped_in: None,
    walk: Walk::Canonical,
    locals: Locals::Slots,
    strings: Strings::Exact,
    numbers: Numbers::Exact,
    property_names: PropertyNames::Verbatim,
    privates: Privates::Verbatim,
    free_names: FreeNames::Verbatim,
    wrapper_spelling: false,
    parent_names: ParentNames::Oxc,
};

/// The naming validator's rename signature (`naming::driver::validate`):
/// the identity key with private names blinded — a renamed output must
/// equal its input up to local and private names.
pub const RENAME_SIGNATURE: CanonicalForm = CanonicalForm {
    name: "rename-signature",
    privates: Privates::Blinded,
    ..IDENTITY_KEY
};

// ---------------------------------------------------------------------------
// The statement walk
// ---------------------------------------------------------------------------

/// The split's statement hash (`hash::statement_hash`): every name masked,
/// literals exact. Version = the split ledger's `hashVersion` (a mismatched
/// ledger is refused, or re-derived through the proven bijection):
///
/// 1 = the TS statement-hash bytes (`src/split/statement-hash.ts`), which
/// every TS-era ledger carries. 2 = the Rust walk's bytes (WP5.6e,
/// 2026-09-25: the structuralSignature exemption ended, the Rust hashes
/// are the only hashes). The two define the same partition on every
/// measured input (the M3 injection's bijection proof), but never the same
/// bytes, so a v1 ledger read as v2 would silently join nothing.
/// 3 = exp094b (2026-10-02): the wrapper-spelling unification reached the
/// statement arm — a SAFE arrow's node line walks under the
/// FunctionExpression token (the SAME shared rule as [`MATCH_KEY`],
/// `hash::wrapper_spelling`), and the function-head fields (`async`,
/// `generator`) ride in the node content. A wrapper a bundler
/// re-serialized arrow -> function keeps its class across the era when
/// spelled as a function; the ARROW side's bytes are new-era (the version
/// gate, not the bytes, carries the refusal — exp093's lesson, mirrored).
pub const STATEMENT: CanonicalForm = CanonicalForm {
    name: "statement",
    version: 3,
    stamped_in: Some("split ledger hashVersion"),
    walk: Walk::Statement,
    locals: Locals::Masked,
    strings: Strings::Exact,
    numbers: Numbers::Exact,
    property_names: PropertyNames::Masked,
    privates: Privates::Masked,
    free_names: FreeNames::Masked,
    wrapper_spelling: true,
    parent_names: ParentNames::Oxc,
};

// ---------------------------------------------------------------------------
// The statement-align walk (close tier, in-run)
// ---------------------------------------------------------------------------

/// The align unit hash and placeholder table (TS `hashPathWithMapping`),
/// also the snap gate's definition hash.
pub const ALIGN_UNIT: CanonicalForm = CanonicalForm {
    name: "align-unit",
    version: 1,
    stamped_in: None,
    walk: Walk::AlignTokens,
    locals: Locals::Slots,
    strings: Strings::Blurred,
    numbers: Numbers::Exact,
    property_names: PropertyNames::Verbatim,
    privates: Privates::Verbatim,
    free_names: FreeNames::Verbatim,
    wrapper_spelling: false,
    parent_names: ParentNames::Babel,
};

/// The alignment GROUP key: [`ALIGN_UNIT`]'s stream with numbers masked
/// (finding #70: a memo cache renumbered on every edit). Derived from the
/// same walk's tokens, not a second walk.
pub const ALIGN_GROUP: CanonicalForm = CanonicalForm {
    name: "align-group",
    numbers: Numbers::Masked,
    ..ALIGN_UNIT
};

/// The switch-case test signature (TS `computeStructuralSignature`):
/// strings exact, so `case "open"` and `case "data"` never cross-pair.
pub const ALIGN_EXACT: CanonicalForm = CanonicalForm {
    name: "align-exact",
    strings: Strings::Exact,
    ..ALIGN_UNIT
};

/// The binding content shingles (TS `computeContentShingles`): strings
/// exact, slot ordinals blinded.
pub const ALIGN_CONTENT: CanonicalForm = CanonicalForm {
    name: "align-content",
    locals: Locals::SlotsUnnumbered,
    strings: Strings::Exact,
    ..ALIGN_UNIT
};

// ---------------------------------------------------------------------------
// The registry
// ---------------------------------------------------------------------------

/// Every form, in this file's order.
pub const FORMS: [&CanonicalForm; 11] = [
    &MATCH_KEY,
    &MATCH_KEY_PRIVATES_BLINDED,
    &LIBRARY_UNITS,
    &VENDOR_CONTENT,
    &IDENTITY_KEY,
    &RENAME_SIGNATURE,
    &STATEMENT,
    &ALIGN_UNIT,
    &ALIGN_GROUP,
    &ALIGN_EXACT,
    &ALIGN_CONTENT,
];

/// The one persisted stamp that is not a serializer form: the library
/// index's literal extractor (`libindex::literals` — a change to any of its
/// rules is a new version; the index manifest records it as `literals/N`).
pub const LIBRARY_LITERALS_VERSION: u64 = 1;

impl CanonicalForm {
    /// The stamp's spelling: `<name>/<version>`.
    pub fn stamp(&self) -> String {
        format!("{}/{}", self.name, self.version)
    }
}
