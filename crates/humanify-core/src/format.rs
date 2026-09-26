//! `core::format` — the native formatter: the TS stage-6 beautify
//! (`src/plugins/babel/babel.ts` → `transformWithPlugins`, i.e. Babel's
//! `transform()` with four plugins and `@babel/generator` 7.29.7)
//! reproduced byte for byte EXCEPT where the TS changed a program's meaning
//! or threw on valid input — those bugs are fixed (findings #42, #44, #45,
//! #46; 00-control §3 "beautifier bugs"; each fixed golden records what it
//! was) — and the one owner of the Babel-shaped AST, its converter and its
//! printer (the `using` desugar prints through it too).
//!
//! The pipeline (plan of record: docs/rust-port/17-formatter-swap.md,
//! WP5.6a + WP5.6b):
//!
//! 1. parse the text as a MODULE (Babel's `transform()` default
//!    `sourceType`, strict mode — [`crate::ingest::Ingest::parse_module`]);
//! 2. convert oxc's AST into Babel's shape ([`convert`], an arena —
//!    [`ast`]) and attach the comments where Babel's parser does
//!    ([`comments`]);
//! 3. run the four plugins' visitors as ONE merged traversal with Babel's
//!    queue/requeue semantics ([`traverse`] is `@babel/traverse`'s engine,
//!    [`beautify`] the twelve visitors, [`scope`] the one scope question
//!    they ask);
//! 4. print with `retainLines: false`, `comments: false` ([`printer`]).
//!
//! Stage 6 of the pipeline ([`format_file`], one call per processed file
//! — WP5.6d). When the file has banner regions it also records the library
//! carry (WP5.6c, finding #32): step 3's OUTPUT tree still holds the raw
//! parse's function nodes with their raw spans ([`ast::Node::span`]), and
//! [`crate::libdetect::function_carry::carry_format_tree`] classifies them
//! per walk ordinal before the printer runs — the TS's
//! `libraryCarryPlugin.post`.

pub mod ast;
pub mod beautify;
pub mod comments;
pub mod convert;
pub mod jsesc;
pub mod printer;
pub mod scope;
pub mod template_raw;
pub mod traverse;

use oxc_allocator::Allocator;

use crate::ingest::Ingest;
use crate::libdetect::CommentRegion;
use crate::libdetect::function_carry::{FunctionLibraryCarry, carry_format_tree};

pub use beautify::Plugins;

/// A planted perturbation (gate red runs only — proves the byte gate can
/// see each mechanism; never set in the pipeline).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Plant {
    /// Run without one visitor (its [`Plugins`] bit).
    DropVisitor(u16),
    /// A replaced node is requeued at the END of its container's queue
    /// (after the remaining siblings) instead of on the priority queue
    /// (right after the current path) — the requeue order reversed.
    RequeueDeferred,
    /// A replaced node is never requeued (never visited again).
    NoRequeue,
    /// A new Scope never crawls its subtree (no path re-parenting).
    NoCrawl,
    /// Print a synthesized number with Rust's `{}` instead of JS
    /// `Number::toString` (`1e21` → `1000000000000000000000`).
    RustNumberFormat,
}

impl Plant {
    /// The CLI spelling: `drop:<visitor>`, `requeue-deferred`,
    /// `rust-number-format`.
    pub fn parse(s: &str) -> Result<Plant, String> {
        if let Some(name) = s.strip_prefix("drop:") {
            return Plugins::bit(name)
                .map(Plant::DropVisitor)
                .ok_or_else(|| format!("unknown visitor {name}"));
        }
        match s {
            "requeue-deferred" => Ok(Plant::RequeueDeferred),
            "no-requeue" => Ok(Plant::NoRequeue),
            "no-crawl" => Ok(Plant::NoCrawl),
            "rust-number-format" => Ok(Plant::RustNumberFormat),
            other => Err(format!("unknown plant {other}")),
        }
    }
}

/// What [`format`] runs.
#[derive(Clone, Copy, Debug)]
pub struct FormatOptions {
    /// The plugins whose visitors run (none = `transformWithPlugins(code,
    /// [])`, the printer-only gate G1).
    pub plugins: Plugins,
    /// A planted perturbation (None in every real run).
    pub plant: Option<Plant>,
}

impl Default for FormatOptions {
    fn default() -> Self {
        FormatOptions {
            plugins: Plugins::STAGE6,
            plant: None,
        }
    }
}

/// Stage 6's output for one file: the formatted text and, when the file
/// has banner regions, the library carry (finding #32).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Formatted {
    pub text: String,
    pub library_carry: Option<FunctionLibraryCarry>,
}

/// Stage 6 for one processed file (`createBabelPlugin()(code, context)`):
/// the formatted text, plus — when `regions` (the file's banner regions,
/// offsets into `code`) is non-empty — the library carry the TS's
/// `libraryCarryPlugin.post` records on the transform's OUTPUT tree.
pub fn format_file(
    code: &str,
    opts: &FormatOptions,
    regions: &[CommentRegion],
) -> Result<Formatted, String> {
    let allocator = Allocator::default();
    let ingest = Ingest::parse_module(&allocator, code);
    if let Some(e) = ingest.errors.first() {
        return Err(format!("parse error: {e}"));
    }
    let mut converter = convert::Converter::new(code);
    let root = converter.file(ingest.program)?;
    let mut tree = converter.tree;
    comments::attach(&mut tree, code, ingest.program, root)?;
    let licenses = comments::license_comments(&tree);
    let mut plugins = opts.plugins;
    if let Some(Plant::DropVisitor(bit)) = opts.plant {
        plugins = Plugins(plugins.0 & !bit);
    }
    if !plugins.is_empty() {
        let undefined_scopes = scope::undefined_binding_scopes(&tree, &ingest)?;
        beautify::run(&mut tree, root, plugins, &undefined_scopes, opts.plant)?;
    }
    // `libraryCarryPlugin.post`: after every visitor, before the printer,
    // on the transformed tree whose function nodes keep their raw spans.
    let library_carry = if regions.is_empty() {
        None
    } else {
        Some(carry_format_tree(&tree, root, regions)?)
    };
    let mut printer = printer::Printer::new(&tree, printer::Mode::Beautify);
    if opts.plant == Some(Plant::RustNumberFormat) {
        printer.plant_rust_numbers();
    }
    Ok(Formatted {
        text: with_license_header(printer.generate(root), &licenses),
        library_carry,
    })
}

/// Finding #46: Babel keeps an `@license` / `@preserve` comment even with
/// `comments: false`, printed where it was attached. Comment PRINTING is
/// not ported, so the documented equivalent is a file HEADER: every such
/// comment, in source order, one per line, before the program (after a
/// `#!` line). The text survives; its position may move to the top — never
/// into code, so the program's meaning cannot change.
fn with_license_header(text: String, licenses: &[String]) -> String {
    if licenses.is_empty() {
        return text;
    }
    let header = licenses.join("\n");
    let split = if text.starts_with("#!") {
        text.find('\n').map_or(text.len(), |i| i + 1)
    } else {
        0
    };
    let (interpreter, program) = text.split_at(split);
    let sep = if interpreter.is_empty() || interpreter.ends_with('\n') {
        ""
    } else {
        "\n"
    };
    if program.is_empty() {
        format!("{interpreter}{sep}{header}")
    } else {
        format!("{interpreter}{sep}{header}\n{program}")
    }
}

/// `transformWithPlugins(code, plugins)` with the stage-6 settings: the
/// formatted text, or the error a Babel throw would be.
pub fn format(code: &str, opts: &FormatOptions) -> Result<String, String> {
    format_file(code, opts, &[]).map(|f| f.text)
}

#[cfg(test)]
mod format_test;

#[cfg(test)]
mod beautify_test;
