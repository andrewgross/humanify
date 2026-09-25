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

use std::path::Path;

use oxc_allocator::Allocator;
use oxc_ast::AstKind;
use oxc_ast::ast::VariableDeclarationKind;

use super::relink::parse_or_err;
use super::scaffold::{js_files_under, read_utf8};

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
fn declares_using(code: &str) -> Result<bool, String> {
    let allocator = Allocator::default();
    let ingest = parse_or_err(&allocator, code)?;
    Ok(ingest.semantic().nodes().iter().any(|n| {
        matches!(
            n.kind(),
            AstKind::VariableDeclaration(d)
                if matches!(d.kind, VariableDeclarationKind::Using | VariableDeclarationKind::AwaitUsing)
        )
    }))
}

/// `desugarUsing(code)`: the transformed text, or None when there is
/// nothing to transform (the caller keeps the original bytes).
pub fn desugar_using(code: &str) -> Result<Option<String>, String> {
    if !has_using_token(code) {
        return Ok(None);
    }
    if !declares_using(code)? {
        return Ok(None);
    }
    Err("using desugar: not yet ported".into())
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
