//! `using` / `await using` desugaring for the runnable split tree — TS
//! `src/split/using-desugar.ts`.
//!
//! Bun cannot `require()` a CommonJS module containing `using`
//! (oven-sh/bun#11100) and Node < 24 cannot parse it, so the runnable tree
//! compiles explicit resource management away. The TS runs Babel's
//! `@babel/plugin-transform-explicit-resource-management` and prints with
//! `@babel/generator` under `retainLines`; only files that really DECLARE
//! with `using` are regenerated (a token prefilter, then a parse-level
//! check), so every other file stays byte-identical.
//!
//! The port reproduces those OUTPUT BYTES through `core::format` (one
//! owner of Babel's AST, converter and generator): `format::convert`
//! builds Babel's AST from oxc's, [`transform`] is the plugin,
//! `format::printer` in `Mode::RetainLines` is the generator (no
//! comments). What it cannot reproduce faithfully it
//! refuses loudly — comments (Babel's attachment is not ported), a helper
//! global shadowed at program scope, an input already spelling a
//! generated uid — the way a Babel throw fails the TS stage.

pub mod helpers;
pub mod transform;

use std::path::Path;

use oxc_allocator::Allocator;
use oxc_ast::AstKind;
use oxc_ast::ast::VariableDeclarationKind;

use super::relink::parse_or_err;
use super::scaffold::{js_files_under, read_utf8};
use crate::format::convert::Converter;
use crate::format::printer::{Mode, Printer};
use crate::ingest::Ingest;
use crate::rename::validated::scopes::BabelScopes;

/// `/\busing\b/` (ASCII word boundaries, as a non-unicode JS regex).
fn has_using_token(code: &str) -> bool {
    let bytes = code.as_bytes();
    let word = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
    let mut from = 0;
    while let Some(i) = code[from..].find("using") {
        let at = from + i;
        let before = at == 0 || !word(bytes[at - 1]);
        let end = at + 5;
        let after = end >= bytes.len() || !word(bytes[end]);
        if before && after {
            return true;
        }
        from = at + 1;
    }
    false
}

/// `declaresUsing(ast)`.
fn declares_using(ingest: &Ingest<'_>) -> bool {
    ingest.semantic().nodes().iter().any(|n| {
        matches!(
            n.kind(),
            AstKind::VariableDeclaration(d)
                if matches!(d.kind, VariableDeclarationKind::Using | VariableDeclarationKind::AwaitUsing)
        )
    })
}

/// The globals the injected helpers read (`helpers-generated.js`
/// metadata): a PROGRAM-scope binding of one of these would make Babel
/// rename that binding file-wide (`addHelper`'s `scope.rename`).
const HELPER_GLOBALS: [&str; 6] = [
    "SuppressedError",
    "Error",
    "Object",
    "TypeError",
    "Symbol",
    "Promise",
];

/// The inputs the port refuses rather than guess at (each fails the
/// finishing stage loudly, as a Babel throw would).
fn refuse(ingest: &Ingest<'_>, code: &str) -> Result<(), String> {
    if !ingest.program.comments.is_empty() {
        return Err("using desugar: comments are not ported (Babel's attachment)".into());
    }
    if code.contains("_usingCtx") || code.contains("_setFunctionName") {
        return Err("using desugar: an identifier spelled like a generated uid".into());
    }
    let scopes = BabelScopes::build(ingest.semantic());
    let program = scopes.program_scope();
    for name in HELPER_GLOBALS {
        if scopes.initial_maps[program.0 as usize]
            .iter()
            .any(|(n, _)| n == name)
        {
            return Err(format!(
                "using desugar: a program-scope binding shadows helper global {name}"
            ));
        }
    }
    Ok(())
}

/// Babel's `transformSync(code, { retainLines: true, compact: false })`
/// with NO plugins: parse, then print. The printer's corpus gate (every
/// file of a tree against the TS's own output) — the desugar prints
/// through the same code.
pub fn print_retaining_lines(code: &str) -> Result<String, String> {
    let allocator = Allocator::default();
    let ingest = parse_or_err(&allocator, code)?;
    if !ingest.program.comments.is_empty() {
        return Err("comments are not ported".into());
    }
    if ingest.program.source_type.is_module() {
        return Err("module source (not a desugar input)".into());
    }
    let mut converter = Converter::new(code);
    let program = converter.program(ingest.program)?;
    Ok(Printer::new(&converter.tree, Mode::RetainLines).generate(program))
}

/// `desugarUsing(code)`: the transformed text, or None when there is
/// nothing to transform (the caller keeps the original bytes).
pub fn desugar_using(code: &str) -> Result<Option<String>, String> {
    if !has_using_token(code) {
        return Ok(None);
    }
    let allocator = Allocator::default();
    let ingest = parse_or_err(&allocator, code)?;
    if !declares_using(&ingest) {
        return Ok(None);
    }
    refuse(&ingest, code)?;
    if ingest.program.source_type.is_module() {
        return Err("module source (not a desugar input)".into());
    }
    let mut converter = Converter::new(code);
    let program = converter.program(ingest.program)?;
    let mut tree = converter.tree;
    transform::transform_program(&mut tree, program)?;
    Ok(Some(
        Printer::new(&tree, Mode::RetainLines).generate(program),
    ))
}

/// `desugarUsingInTree(outputDir)`: the number of files rewritten.
pub fn desugar_using_in_tree(output_dir: &Path) -> Result<usize, String> {
    let mut transformed = 0;
    for file in js_files_under(output_dir)? {
        let code = read_utf8(&file)?;
        let Some(out) = desugar_using(&code).map_err(|e| format!("{}: {e}", file.display()))?
        else {
            continue;
        };
        std::fs::write(&file, out).map_err(|e| format!("write {}: {e}", file.display()))?;
        transformed += 1;
    }
    Ok(transformed)
}

/// `desugarSummary(outputDir, count)`.
pub fn desugar_summary(output_dir: &Path, count: usize) -> String {
    if count > 0 {
        let base = output_dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        format!(
            "Desugared `using` in {count} file(s) under {base} (Bun cannot require CJS+using, bun#11100; Node < 24 cannot parse it)"
        )
    } else {
        "No `using` declarations to desugar".into()
    }
}

#[cfg(test)]
mod using_test;
