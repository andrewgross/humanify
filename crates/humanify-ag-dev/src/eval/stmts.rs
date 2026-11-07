//! The scorer's statement extraction — the ONE place an emitted JavaScript
//! text becomes statements (TS: `experiments/lib/trees.ts`
//! `bundleStatements` / `fileStatements`, `034/statements.ts`,
//! `037/diff-composition.ts` `statementsOf`).
//!
//! Two input shapes, each stated and refused when given the other:
//!
//! - a BUNDLE (`.humanify/humanified.js`) wraps everything in an IIFE, so
//!   its statements are the wrapper body's ([`bundle_statements`]); a text
//!   with no wrapper is an ERROR, never "one giant statement";
//! - a SPLIT FILE (`src/**.js`, `vendor/**.js`) is a plain module, so its
//!   statements are the program body's ([`file_statements`]).
//!
//! Parsing is the pipeline's own (`humanify_ag_core::ingest`, Babel's
//! `unambiguous` rule: script first, module when only that parses), the
//! wrapper is the pipeline's own detection
//! (`humanify_ag_core::modules::wrapper`), and every statement's identity
//! is the KPI form of the statement walk (`hash::forms::KPI_STATEMENT`,
//! scorer version 2: every name masked, the wrapper spelling kept apart).
//! A parse error is FATAL: a broken emitted file must never score as
//! genuine change.

use humanify_ag_core::hash::forms::{KPI_STATEMENT, STATEMENT};
use humanify_ag_core::hash::statement_hash::statement_hash_in;
use humanify_ag_core::ingest::{Ingest, parse_estree_json};
use humanify_ag_core::modules::wrapper::find_wrapper_function;
use oxc_allocator::Allocator;
use oxc_ast::AstKind;
use oxc_ast::ast::{Expression, Statement};
use oxc_estree::{CompactSerializer, ESTree};
use oxc_span::GetSpan;

/// One wrapper-body statement of a bundle.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stmt {
    /// The KPI statement hash (16 hex).
    pub hash: String,
    pub text: String,
    /// Lines of `text` (`text.split("\n").length`; 0 for empty text).
    pub lines: usize,
    /// The binding names the statement declares, each declaration once
    /// (function ids and params, declarator ids, class ids, catch params):
    /// only when asked for (the mint attribution reads them).
    pub declared: Vec<String>,
}

/// One top-level statement of a split file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileStmt {
    /// The KPI statement hash.
    pub hash: String,
    /// The statement under the split's `STATEMENT` form, which unifies the
    /// wrapper spelling (`hash::wrapper_spelling`, the ONE rule): two
    /// statements equal here and different under `hash` are the same code
    /// modulo arrow <-> function spelling.
    pub spelling_hash: String,
    pub text: String,
    /// `const x = require("...")`: an import header line.
    pub is_require: bool,
}

impl FileStmt {
    /// The text's lines (`text.split("\n")`; none for empty text).
    pub fn lines(&self) -> Vec<&str> {
        split_lines(&self.text)
    }
}

/// `text.split("\n")`, but no lines for an empty text.
pub fn split_lines(text: &str) -> Vec<&str> {
    if text.is_empty() {
        Vec::new()
    } else {
        text.split('\n').collect()
    }
}

/// `text.split("\n").length`, but 0 for an empty text.
pub fn line_count(text: &str) -> usize {
    if text.is_empty() {
        0
    } else {
        text.bytes().filter(|b| *b == b'\n').count() + 1
    }
}

fn parse<'a>(allocator: &'a Allocator, code: &'a str, label: &str) -> Result<Ingest<'a>, String> {
    let ingest = Ingest::parse_unambiguous(allocator, code);
    if let Some(first) = ingest.errors.first() {
        return Err(format!(
            "{label}: parse failed ({} error(s); first: {})",
            ingest.errors.len(),
            first.lines().next().unwrap_or_default()
        ));
    }
    Ok(ingest)
}

/// The ESTree JSON text of each statement (spans as byte offsets).
fn statement_json(stmts: &[&Statement<'_>]) -> Vec<String> {
    stmts
        .iter()
        .map(|s| {
            let mut ser = CompactSerializer::new(false, true);
            s.serialize(&mut ser);
            ser.into_string()
        })
        .collect()
}

/// Top-level statements of a BUNDLE: the wrapper IIFE's body. Throws when
/// the text has no wrapper. `declared` asks for each statement's declared
/// binding names.
pub fn bundle_statements(code: &str, label: &str, declared: bool) -> Result<Vec<Stmt>, String> {
    let allocator = Allocator::default();
    let ingest = parse(&allocator, code, label)?;
    let semantic = ingest.semantic();
    let wrapper = find_wrapper_function(ingest.program, semantic).ok_or_else(|| {
        format!(
            "{label}: no wrapper IIFE found. This reads BUNDLES (.humanify/humanified.js); \
             an emitted split file is read by file_statements"
        )
    })?;
    let body = semantic
        .nodes()
        .iter()
        .find_map(|n| match n.kind() {
            AstKind::FunctionBody(b) if b.span == wrapper.body_span => Some(b),
            _ => None,
        })
        .ok_or_else(|| format!("{label}: the wrapper's body block is missing"))?;
    let stmts: Vec<&Statement<'_>> = body.statements.iter().collect();
    let spans: Vec<(u32, u32)> = stmts
        .iter()
        .map(|s| (s.span().start, s.span().end))
        .collect();
    let names = if declared {
        declared_by_statement(&ingest, &spans)
    } else {
        vec![Vec::new(); spans.len()]
    };
    let jsons = statement_json(&stmts);
    let hashes = humanify_ag_core::par::map_ordered(&jsons, |j| {
        statement_hash_in(&KPI_STATEMENT, &parse_estree_json(j))
    });
    Ok(spans
        .iter()
        .zip(hashes)
        .zip(names)
        .map(|((&(start, end), hash), declared)| {
            let text = code[start as usize..end as usize].to_string();
            Stmt {
                hash,
                lines: line_count(&text),
                text,
                declared,
            }
        })
        .collect())
}

/// Each statement's declared binding names: every binding identifier whose
/// position lies inside the statement's span (statements do not overlap).
fn declared_by_statement(ingest: &Ingest<'_>, spans: &[(u32, u32)]) -> Vec<Vec<String>> {
    let mut idents: Vec<(u32, String)> = ingest
        .semantic()
        .nodes()
        .iter()
        .filter_map(|n| match n.kind() {
            AstKind::BindingIdentifier(id) => Some((id.span.start, id.name.to_string())),
            _ => None,
        })
        .collect();
    idents.sort_by_key(|(start, _)| *start);
    spans
        .iter()
        .map(|&(start, end)| {
            let from = idents.partition_point(|(s, _)| *s < start);
            let to = idents.partition_point(|(s, _)| *s < end);
            idents[from..to].iter().map(|(_, n)| n.clone()).collect()
        })
        .collect()
}

/// `const x = require("...")` — one declarator whose init is a call of
/// the free `require` with one string argument.
fn is_require_decl(s: &Statement<'_>) -> bool {
    let Statement::VariableDeclaration(d) = s else {
        return false;
    };
    if d.declarations.len() != 1 {
        return false;
    }
    let Some(init) = d.declarations[0].init.as_ref() else {
        return false;
    };
    let Expression::CallExpression(call) = humanify_ag_core::babel_view::unparen(init) else {
        return false;
    };
    matches!(&call.callee, Expression::Identifier(id) if id.name == "require")
        && call.arguments.len() == 1
        && matches!(
            call.arguments[0].as_expression(),
            Some(Expression::StringLiteral(_))
        )
}

/// Top-level statements of an emitted SPLIT FILE (a plain module body),
/// each with both statement-walk hashes.
pub fn file_statements(code: &str, label: &str) -> Result<Vec<FileStmt>, String> {
    let allocator = Allocator::default();
    let ingest = parse(&allocator, code, label)?;
    let stmts: Vec<&Statement<'_>> = ingest.program.body.iter().collect();
    let jsons = statement_json(&stmts);
    Ok(stmts
        .iter()
        .zip(jsons)
        .map(|(s, json)| {
            let value = parse_estree_json(&json);
            let span = s.span();
            FileStmt {
                hash: statement_hash_in(&KPI_STATEMENT, &value),
                spelling_hash: statement_hash_in(&STATEMENT, &value),
                text: code[span.start as usize..span.end as usize].to_string(),
                is_require: is_require_decl(s),
            }
        })
        .collect())
}

#[cfg(test)]
#[path = "stmts_test.rs"]
mod stmts_test;
