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
//! The port builds the plugin's PROGRAM but does not reprint the file
//! (exp228): [`transform`] plans the plugin's edits as splices into the
//! file's own text, so every line the desugar does not have to change
//! keeps the writer's bytes, and a file that starts or stops declaring
//! with `using` between two versions changes only the lines it must.
//! `core::format` still parses (`format::convert` builds Babel's AST from
//! oxc's, which the plan walks) and prints the two injected helpers
//! (`format::printer` in `Mode::RetainLines`). What it cannot do
//! faithfully it refuses loudly — a helper global shadowed at program
//! scope, an input already spelling a generated uid — the way a Babel
//! throw fails the TS stage.

pub mod helpers;
pub mod transform;

use helpers::helper_declaration;

use std::path::{Path, PathBuf};

use oxc_allocator::Allocator;
use oxc_ast::AstKind;
use oxc_ast::ast::VariableDeclarationKind;

use super::relink::parse_or_err;
use super::scaffold::js_files_under;
use crate::error::FileError;
use crate::format::FormatError;
use crate::format::ast::{Kind, Tree};
use crate::format::convert::Converter;
use crate::format::printer::{Mode, Printer};
use crate::ingest::{Ingest, ParseError};
use crate::output_tree::OutputTree;
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

/// Why the `using` desugar refused a file.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum UsingError {
    /// The file does not parse.
    #[error(transparent)]
    Parse(#[from] ParseError),
    /// The formatter's converter refused the file.
    #[error(transparent)]
    Format(#[from] FormatError),
    /// An input the port refuses rather than guess at.
    #[error("{0}")]
    Refused(&'static str),
    /// Two splices overlap, or one falls outside the text.
    #[error("using desugar: overlapping splice at {at}")]
    OverlappingSplice { at: usize },
    /// A program-scope binding shadows a global the helpers read.
    #[error("using desugar: a program-scope binding shadows helper global {name}")]
    ShadowedGlobal { name: &'static str },
    /// A helper the desugar does not carry.
    #[error("unknown Babel helper {name}")]
    UnknownHelper { name: String },
    /// A carried helper's source does not parse.
    #[error("helper {name} does not parse")]
    HelperParse { name: String },
}

/// Why desugaring an output tree failed.
#[derive(Debug, thiserror::Error)]
pub enum UsingTreeError {
    /// Listing, reading or writing a file failed.
    #[error(transparent)]
    File(#[from] FileError),
    /// The desugar refused one file.
    #[error("{}: {source}", path.display())]
    InFile { path: PathBuf, source: UsingError },
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
/// finishing stage loudly, as a Babel throw would). Comments are kept:
/// the splices never move them.
fn refuse(ingest: &Ingest<'_>, code: &str) -> Result<(), UsingError> {
    if code.contains("_usingCtx") || code.contains("_setFunctionName") {
        return Err(UsingError::Refused(
            "using desugar: an identifier spelled like a generated uid",
        ));
    }
    let scopes = BabelScopes::build(ingest.semantic());
    let program = scopes.program_scope();
    for name in HELPER_GLOBALS {
        if scopes.initial_maps[program.0 as usize]
            .iter()
            .any(|(n, _)| n == name)
        {
            return Err(UsingError::ShadowedGlobal { name });
        }
    }
    Ok(())
}

/// Babel's `transformSync(code, { retainLines: true, compact: false })`
/// with NO plugins: parse, then print — the printer the desugar prints
/// through, exercised alone by the unit tests.
#[cfg(test)]
pub(crate) fn print_retaining_lines(code: &str) -> Result<String, UsingError> {
    let allocator = Allocator::default();
    let ingest = parse_or_err(&allocator, code)?;
    if !ingest.program.comments.is_empty() {
        return Err(UsingError::Refused("comments are not ported"));
    }
    if ingest.program.source_type.is_module() {
        return Err(UsingError::Refused("module source (not a desugar input)"));
    }
    let mut converter = Converter::new(code);
    let program = converter.program(ingest.program)?;
    Ok(Printer::new(&converter.tree, Mode::RetainLines).generate(program))
}

/// The helpers' declarations (`addHelper` order; Babel unshifts each, so
/// the last added comes first), printed as Babel prints them: compact, on
/// one line.
fn helpers_text(tree: &mut Tree, helpers: &[(&'static str, String)]) -> Result<String, UsingError> {
    let mut body = Vec::new();
    for (name, uid) in helpers {
        body.insert(0, helper_declaration(tree, name, uid)?);
    }
    let program = tree.synth(Kind::Program {
        interpreter: None,
        directives: Vec::new(),
        body,
    });
    Ok(Printer::new(tree, Mode::RetainLines).generate(program))
}

/// `desugarUsing(code)`: the transformed text, or None when there is
/// nothing to transform (the caller keeps the original bytes).
///
/// The result is the input with SPLICES applied ([`transform`]): the
/// helpers on a new first line, the `try` opener and closer on the lines
/// of each block's braces, `const` and the context call on each `using`
/// statement. Nothing else is reprinted, so a file that starts or stops
/// declaring with `using` changes only those lines.
pub fn desugar_using(code: &str) -> Result<Option<String>, UsingError> {
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
        return Err(UsingError::Refused("module source (not a desugar input)"));
    }
    let mut converter = Converter::new(code);
    let program = converter.program(ingest.program)?;
    let mut tree = converter.tree;
    let mut state = transform::State::new(code);
    transform::plan_program(&mut tree, program, &mut state)?;
    let helpers = helpers_text(&mut tree, &state.helpers)?;
    let mut splices = std::mem::take(&mut state.splices);
    splices.push(transform::helpers_splice(&tree, program, helpers)?);
    transform::apply(code, splices).map(Some)
}

/// `desugarUsingInTree(outputDir)`: the number of files rewritten, over
/// every `.js` / `.cjs` file of the output folder as it stands (a file an
/// earlier run left there included, as on disk).
pub fn desugar_using_in_tree(tree: &mut OutputTree) -> Result<usize, UsingTreeError> {
    let mut transformed = 0;
    for file in js_files_under(tree)? {
        let code = tree.read_lossy(&file)?;
        let Some(out) = desugar_using(&code).map_err(|source| UsingTreeError::InFile {
            path: file.clone(),
            source,
        })?
        else {
            continue;
        };
        tree.put(file, out);
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
