//! Ingest (WP1.2): oxc parse + semantic analysis over the TS-beautified
//! text (docs/rust-port/10-work-breakdown.md §2 WP1.2).
//!
//! During phases 1-5a the Rust pipeline consumed TS-beautified text (the
//! `--beautified-input` ingestion decision, 00-control §3); since WP5.6d it
//! parses the native formatter's output (`crate::format`, byte-identical to
//! the TS beautify). The ingest owns:
//! the parse, the semantic build (symbols/scopes/references — oxc's model
//! IS our identity model, 02 §1), and the arena that holds the AST for the
//! run. Span UNIT: oxc spans are UTF-8 byte offsets natively — matching
//! the dump's decided unit (07 §1) — so ingest spans need no conversion
//! while the input is the TS-beautified text.
//!
//! WP1.2's gate: parse all four oracle pairs' beautified text with zero
//! errors; symbol/scope counts recorded (the counts table lands in the
//! merge message, Babel's counts beside them for the first comparison).

use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_semantic::{Semantic, SemanticBuilder};
use oxc_span::SourceType;

/// What one successful ingest carries. The arena is the ONE arena ever
/// (02 §6): the fresh bundle's; the caller holds it for the run.
pub struct Ingest<'a> {
    /// The text ingested (the TS-beautified bundle). Kept alongside the AST:
    /// every span indexes into it, and the canonical-text tables are built
    /// from it (07 §1's anchored texts).
    pub text: &'a str,
    /// The program, allocated INTO the arena so every structure hangs off
    /// one lifetime (02 §6: the arena is held for the run and dropped after
    /// the final render).
    pub program: &'a oxc_ast::ast::Program<'a>,
    /// PRIVATE, read through [`Ingest::semantic`] only: the shared-accessor
    /// wrapper that makes "no module outside `core::emit` holds
    /// `&mut Scoping`" a compile-checked fact (02 §2, WP3.1's gate). oxc's
    /// `Semantic::scoping_mut` / `Scoping::set_symbol_name` are public and
    /// cannot be resealed, so the only way to keep names out of reach is to
    /// never hand out `&mut Semantic`. Names change through the validated
    /// rename overlay (`rename::validated`); the render applies them once.
    ///
    /// ```compile_fail
    /// # use humanify_ag_core::ingest::Ingest;
    /// # let allocator = oxc_allocator::Allocator::default();
    /// let mut ingest = Ingest::parse(&allocator, "var a;", "t.js");
    /// // E0616: the field is private — no path to `scoping_mut`.
    /// let _scoping = ingest.semantic().scoping_mut();
    /// ```
    semantic: Semantic<'a>,
    /// Parse diagnostics: EMPTY for a clean ingest; anything here is a
    /// loud failure.
    pub errors: Vec<String>,
}

/// The program's ESTree JSON, parsed: `program.to_estree_json(false,
/// true)` (no TS fields; `start`/`end` plus a `range` pair on every node —
/// the walks' skip lists drop `range`). ONE owner, so a side is serialized
/// and parsed once and every consumer shares the tree.
pub fn program_estree_json(program: &oxc_ast::ast::Program<'_>) -> serde_json::Value {
    parse_estree_json(&program.to_estree_json(false, true))
}

/// Parse oxc's ESTree JSON text. The AST nests hundreds deep, past
/// serde_json's default recursion limit; unbounded depth is safe (the
/// input is oxc's own serialization of a program that parsed). Malformed
/// text answers `Null` — use [`try_parse_estree_json`] where a reader must
/// say why.
pub fn parse_estree_json(text: &str) -> serde_json::Value {
    try_parse_estree_json(text).unwrap_or(serde_json::Value::Null)
}

/// [`parse_estree_json`], with the parse error. ONE owner for reading
/// oxc's ESTree JSON back.
///
/// oxc's JSON follows `JSON.parse`, which is looser than serde_json in
/// two places that real JavaScript reaches (exp219), and either used to
/// turn a WHOLE program into `Null`:
/// - a string holding a lone surrogate (`"\ud800"`, or a tagged template's
///   cooked text over a surrogate range) — legal JavaScript, not legal
///   Unicode. serde_json refuses the `\uXXXX` escape. It is read as U+FFFD
///   followed by the code unit's four hex digits, oxc's own in-memory
///   encoding of a lone surrogate, so each code unit stays distinct.
/// - a number past the double range (`1e400` is `Infinity`): oxc writes
///   `1e+400`, which serde_json refuses as out of range. It is read as the
///   largest finite double, still a NUMBER (readers classify literals by
///   their JSON type); the literal's `raw` keeps the source spelling.
pub fn try_parse_estree_json(text: &str) -> Result<serde_json::Value, String> {
    let text = strict_json_from_estree(text);
    let mut de = serde_json::Deserializer::from_str(&text);
    de.disable_recursion_limit();
    serde::Deserialize::deserialize(&mut de).map_err(|e| format!("estree json: {e}"))
}

/// oxc's spelling of an infinite number (`oxc_estree`'s `f64` impl).
const ESTREE_INFINITY: &[u8] = b"1e+400";
/// What [`try_parse_estree_json`] reads it as: `f64::MAX`.
const FINITE_STAND_IN: &str = "1.7976931348623157e308";

/// The ESTree JSON with every lone-surrogate escape and every infinite
/// number rewritten into JSON serde_json accepts (see
/// [`try_parse_estree_json`]); borrowed untouched when the text holds
/// neither (the common case — one substring scan).
fn strict_json_from_estree(text: &str) -> std::borrow::Cow<'_, str> {
    let might_need = text.contains("\\ud") || text.contains("\\uD") || text.contains("e+400");
    if !might_need {
        return std::borrow::Cow::Borrowed(text);
    }
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len() + 64);
    let mut copied = 0;
    let mut i = 0;
    let mut in_string = false;
    while i < bytes.len() {
        let step = if in_string {
            string_step(bytes, i, &mut in_string)
        } else {
            value_step(bytes, i, &mut in_string)
        };
        match step {
            Step::Advance(n) => i += n,
            Step::Replace(n, with) => {
                out.push_str(&text[copied..i]);
                out.push_str(&with);
                i += n;
                copied = i;
            }
        }
    }
    if copied == 0 {
        return std::borrow::Cow::Borrowed(text);
    }
    out.push_str(&text[copied..]);
    std::borrow::Cow::Owned(out)
}

/// One scanner step: skip `n` bytes, or replace them.
enum Step {
    Advance(usize),
    Replace(usize, String),
}

/// A step inside a JSON string: an escape is consumed whole (so an escaped
/// backslash never starts a `\u`), a valid surrogate PAIR is left alone,
/// and a lone surrogate escape becomes U+FFFD plus its four hex digits.
fn string_step(bytes: &[u8], i: usize, in_string: &mut bool) -> Step {
    match bytes[i] {
        b'"' => {
            *in_string = false;
            Step::Advance(1)
        }
        b'\\' => match unicode_escape(bytes, i) {
            Some(unit) if (0xD800..0xDC00).contains(&unit) => match unicode_escape(bytes, i + 6) {
                Some(low) if (0xDC00..0xE000).contains(&low) => Step::Advance(12),
                _ => Step::Replace(6, format!("\\ufffd{unit:04x}")),
            },
            Some(unit) if (0xDC00..0xE000).contains(&unit) => {
                Step::Replace(6, format!("\\ufffd{unit:04x}"))
            }
            Some(_) => Step::Advance(6),
            None => Step::Advance(2),
        },
        _ => Step::Advance(1),
    }
}

/// A step outside every string: a string opens, or an infinite number
/// token (`1e+400`, not the tail of a longer number) becomes the finite
/// stand-in; a sign before it is kept.
fn value_step(bytes: &[u8], i: usize, in_string: &mut bool) -> Step {
    match bytes[i] {
        b'"' => {
            *in_string = true;
            Step::Advance(1)
        }
        b'1' if bytes[i..].starts_with(ESTREE_INFINITY)
            && !i
                .checked_sub(1)
                .is_some_and(|p| bytes[p].is_ascii_digit() || bytes[p] == b'.')
            && !bytes
                .get(i + ESTREE_INFINITY.len())
                .is_some_and(u8::is_ascii_digit) =>
        {
            Step::Replace(ESTREE_INFINITY.len(), FINITE_STAND_IN.to_string())
        }
        _ => Step::Advance(1),
    }
}

/// The code unit of a `\uXXXX` escape starting at `i`, if there is one.
fn unicode_escape(bytes: &[u8], i: usize) -> Option<u32> {
    if bytes.get(i) != Some(&b'\\') || bytes.get(i + 1) != Some(&b'u') {
        return None;
    }
    let hex = std::str::from_utf8(bytes.get(i + 2..i + 6)?).ok()?;
    u32::from_str_radix(hex, 16).ok()
}

impl<'a> Ingest<'a> {
    /// The semantic model, shared (see the field's docs): the ONLY accessor.
    pub fn semantic(&self) -> &Semantic<'a> {
        &self.semantic
    }

    /// Parse + build semantic over the given text. `source_type` is derived
    /// from the path when given (jsx/mjs flags matter for the bundles).
    pub fn parse(allocator: &'a Allocator, text: &'a str, source_name: &str) -> Ingest<'a> {
        let source_type = SourceType::from_path(source_name)
            .unwrap_or_default()
            .with_script(true);
        Ingest::parse_as(allocator, text, source_type)
    }

    /// Babel `transform()`'s parse: no `sourceType` given, so Babel's
    /// default `"module"` (strict mode) — the stage-6 beautify's parse
    /// quirk (17-formatter-swap §1.1), NOT the pipeline's `"unambiguous"`.
    ///
    /// Babel's parser also raises the early errors oxc leaves to its
    /// semantic checker (legacy octal and `with` in strict mode, an export
    /// of an undeclared name, a redeclaration), and a Babel throw fails the
    /// TS stage — so this parse runs the checker and reports them too.
    pub fn parse_module(allocator: &'a Allocator, text: &'a str) -> Ingest<'a> {
        Ingest::parse_checked(allocator, text, SourceType::mjs(), true)
    }

    /// Parse + build semantic with an explicit source type.
    fn parse_as(allocator: &'a Allocator, text: &'a str, source_type: SourceType) -> Ingest<'a> {
        Ingest::parse_checked(allocator, text, source_type, false)
    }

    fn parse_checked(
        allocator: &'a Allocator,
        text: &'a str,
        source_type: SourceType,
        check_syntax: bool,
    ) -> Ingest<'a> {
        let ret = Parser::new(allocator, text, source_type).parse();

        let mut errors: Vec<String> = ret.diagnostics.iter().map(|e| format!("{e}")).collect();

        // Move the program into the arena; the semantic build borrows from
        // there, so everything hangs off 'a.
        let program: &'a oxc_ast::ast::Program<'a> = allocator.alloc(ret.program);
        let built = SemanticBuilder::new()
            .with_build_nodes(true)
            .with_check_syntax_error(check_syntax)
            .build(program);
        if check_syntax {
            errors.extend(built.diagnostics.iter().map(|e| format!("{e}")));
        }
        let semantic = built.semantic;

        Ingest {
            text,
            program,
            semantic,
            errors,
        }
    }

    /// Babel's `sourceType: "unambiguous"` (the TS `parseSourceAst` default,
    /// which the unpack classifier parses the RAW bundle with): a script
    /// unless the text only parses as a module (import/export,
    /// `import.meta`). Parsed as a script first; a script parse with
    /// diagnostics is retried as a module and the module parse is taken
    /// when IT is clean. The real bundles are CJS-wrapped scripts; the ESM
    /// shape is the synthetic fixtures' `import{createRequire…}` head.
    pub fn parse_unambiguous(allocator: &'a Allocator, text: &'a str) -> Ingest<'a> {
        let script = Ingest::parse(allocator, text, "input.js");
        if script.errors.is_empty() {
            return script;
        }
        let module = Ingest::parse_as(allocator, text, SourceType::mjs());
        if module.errors.is_empty() {
            module
        } else {
            script
        }
    }
}
