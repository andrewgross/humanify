//! `core::format` — the native formatter: the TS stage-6 beautify
//! (`src/plugins/babel/babel.ts` → `transformWithPlugins`, i.e. Babel's
//! `transform()` with four plugins and `@babel/generator` 7.29.7)
//! reproduced BYTE FOR BYTE, bugs included (finding #42; 00-control §3
//! "beautifier bugs"), and the one owner of the Babel-shaped AST, its
//! converter and its printer (the `using` desugar prints through it too).
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
//! Not wired into the pipeline yet (WP5.6d); the library carry (WP5.6c)
//! reads function RAW starts from [`ast::Node::span`] — the extension
//! point is left in place, unused here.

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

/// `transformWithPlugins(code, plugins)` with the stage-6 settings: the
/// formatted text, or the error a Babel throw would be.
pub fn format(code: &str, opts: &FormatOptions) -> Result<String, String> {
    let allocator = Allocator::default();
    let ingest = Ingest::parse_module(&allocator, code);
    if let Some(e) = ingest.errors.first() {
        return Err(format!("parse error: {e}"));
    }
    let mut converter = convert::Converter::new(code);
    let root = converter.file(ingest.program)?;
    let mut tree = converter.tree;
    comments::attach(&mut tree, code, ingest.program, root)?;
    if let Some(c) = comments::printable(&tree) {
        // `shouldPrintComment` keeps `@license` / `@preserve` comments even
        // with `comments: false`; printing comments is not ported.
        return Err(format!(
            "an @license/@preserve comment at {} would be printed (comment printing is not ported)",
            c.start
        ));
    }
    let mut plugins = opts.plugins;
    if let Some(Plant::DropVisitor(bit)) = opts.plant {
        plugins = Plugins(plugins.0 & !bit);
    }
    if !plugins.is_empty() {
        let undefined_scopes = scope::undefined_binding_scopes(&tree, &ingest)?;
        beautify::run(&mut tree, root, plugins, &undefined_scopes, opts.plant)?;
    }
    let mut printer = printer::Printer::new(&tree, printer::Mode::Beautify);
    if opts.plant == Some(Plant::RustNumberFormat) {
        printer.plant_rust_numbers();
    }
    Ok(printer.generate(root))
}

#[cfg(test)]
mod format_test;
