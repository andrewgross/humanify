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
    /// # use humanify_core::ingest::Ingest;
    /// # let allocator = oxc_allocator::Allocator::default();
    /// let mut ingest = Ingest::parse(&allocator, "var a;", "t.js");
    /// // E0616: the field is private — no path to `scoping_mut`.
    /// let _scoping = ingest.semantic().scoping_mut();
    /// ```
    semantic: Semantic<'a>,
    /// Parse diagnostics: EMPTY for a clean ingest; anything here is a
    /// loud failure (the gate's "zero errors").
    pub errors: Vec<String>,
}

/// The recordable counts (the WP1.2 gate's table).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct IngestCounts {
    pub symbols: usize,
    pub scopes: usize,
    pub references: usize,
    pub top_level_statements: usize,
    /// The wrapper IIFE body's statement count — the split's unit and the
    /// number the oracle comparison is actually about.
    pub wrapper_statements: usize,
    pub text_bytes: usize,
}

/// Count the statements inside the top-level wrapper call/arrow, unwrapping
/// oxc's ParenthesizedExpression nodes (R1's finding).
pub fn wrapper_statement_count(program: &oxc_ast::ast::Program<'_>) -> usize {
    let Some(stmt) = program.body.first() else {
        return 0;
    };
    let mut expr: Option<&oxc_ast::ast::Expression> = match stmt {
        oxc_ast::ast::Statement::ExpressionStatement(es) => Some(&es.expression),
        _ => None,
    };
    if let Some(e) = expr {
        // (the shared paren view — Babel drops the wrappers)
        expr = Some(crate::babel_view::unparen(e));
    }
    match expr {
        Some(oxc_ast::ast::Expression::ArrowFunctionExpression(a)) => match &a.body {
            oxc_ast::ast::ArrowFunctionBody::FunctionBody(b) => b.statements.len(),
            _ => 0,
        },
        Some(oxc_ast::ast::Expression::FunctionExpression(f)) => {
            f.body.as_ref().map(|b| b.statements.len()).unwrap_or(0)
        }
        Some(oxc_ast::ast::Expression::CallExpression(c)) => {
            // The callee may be parenthesized: `(function(){...})()`.
            // (the shared paren view — Babel drops the wrappers)
            let callee = crate::babel_view::unparen(&c.callee);
            match callee {
                oxc_ast::ast::Expression::ArrowFunctionExpression(a) => match &a.body {
                    oxc_ast::ast::ArrowFunctionBody::FunctionBody(b) => b.statements.len(),
                    _ => 0,
                },
                oxc_ast::ast::Expression::FunctionExpression(f) => {
                    f.body.as_ref().map(|b| b.statements.len()).unwrap_or(0)
                }
                _ => 0,
            }
        }
        _ => 0,
    }
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
/// text answers `Null`.
pub fn parse_estree_json(text: &str) -> serde_json::Value {
    let mut de = serde_json::Deserializer::from_str(text);
    de.disable_recursion_limit();
    serde::Deserialize::deserialize(&mut de).unwrap_or(serde_json::Value::Null)
}

/// The WP1.2 gate helper: counts + the parse errors of one text.
pub fn ingest_counts_of_file(text: &str, name: &str) -> (IngestCounts, Vec<String>) {
    let allocator = Allocator::default();
    let ingest = Ingest::parse(&allocator, text, name);
    let mut counts = ingest.counts();
    counts.wrapper_statements = wrapper_statement_count(ingest.program);
    (counts, ingest.errors)
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

    /// The gate's counts.
    pub fn counts(&self) -> IngestCounts {
        let scoping = self.semantic.scoping();
        IngestCounts {
            symbols: scoping.symbol_ids().len(),
            scopes: scoping.scope_descendants_from_root().len(),
            references: scoping.references_len(),
            top_level_statements: self.program.body.len(),
            wrapper_statements: wrapper_statement_count(self.program),
            text_bytes: self.text.len(),
        }
    }
}
