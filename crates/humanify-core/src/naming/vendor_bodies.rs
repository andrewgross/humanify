//! The vendored factory bodies' identifiers, names mode only (exp169):
//! "restore or ask, never freeze" (exp166) extended to the bodies the
//! unpack extracts into `vendor/`. A body's bindings the index restored
//! keep their names (held by the era's one hold rule); every other one is
//! named exactly like app code — the prior release's carry first (exact,
//! then close match), then the model.
//!
//! ONE path: the bodies do not get a namer of their own. They are joined
//! into one text — each body one top-level statement `(<body>);`, in
//! vendor-path order (paths are stable across releases, so the two sides'
//! orders agree) — formatted, and named by [`crate::naming::driver::run_naming`]
//! with the prior release's joined text ([`VENDOR_NAMING_SOURCE`]) as its
//! prior. The named text is split back into one body per file. The
//! module identifiers, the file paths and the vendor record are not
//! touched: a body's free identifiers (the cross-module references, the
//! captures the finish's relink bridges) are never renamed.

use std::collections::BTreeSet;

use humanify_model::llm::NameProvider;

use crate::artifact_dump::DispatchLog;
use crate::naming::driver::{NamingConfig, NamingInput, NamingOutcome, run_naming};

/// The joined, named bodies of one run: the next release's prior for this
/// pass, beside `.humanify/humanified.js` (the era's).
pub const VENDOR_NAMING_SOURCE: &str = ".humanify/vendor-humanified.js";

/// One vendored factory body as the unpack wrote it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VendorBody {
    /// The file, relative to the output directory (`vendor/…`).
    pub path: String,
    /// The body's text (a function expression).
    pub text: String,
    /// The names the index restored inside it (Site B).
    pub restored: BTreeSet<String>,
}

/// What the pass produced.
pub struct VendorNaming {
    /// Each body's named text, in the INPUT order.
    pub texts: Vec<String>,
    /// The joined named text — written to [`VENDOR_NAMING_SOURCE`].
    pub joined: String,
    /// Whether the prior was used (a prior the driver refused as another
    /// program is dropped and the bodies are named fresh).
    pub prior_used: bool,
    /// Why the driver refused the prior, when it did.
    pub prior_refusal: Option<String>,
    /// The bodies left as extracted, with why (path order).
    pub skipped: Vec<(String, Skip)>,
    pub outcome: NamingOutcome,
}

/// Why a body is left as the unpack extracted it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Skip {
    /// The formatter's strict parse refused it.
    Unformattable,
    /// It holds a lone UTF-16 surrogate escape (`"\uD800"`): the ESTree
    /// serialization every side of the match reads cannot carry one, so
    /// one such body would cost every OTHER body its prior.
    LoneSurrogate,
    /// A formatted line longer than [`MAX_LINE_CHARS`]: no prompt window
    /// can show it inside the model's context (highlight.js keyword
    /// tables, symbol lists — 7 calls refused at 33–84k tokens).
    LongLine,
}

impl Skip {
    pub fn as_str(self) -> &'static str {
        match self {
            Skip::Unformattable => "unformattable",
            Skip::LoneSurrogate => "lone-surrogate",
            Skip::LongLine => "long-line",
        }
    }
}

/// The longest formatted line a named body may hold (chars).
pub const MAX_LINE_CHARS: usize = 20_000;

/// The joined text: one `(<body>);` statement per body, in the given order.
pub fn join_bodies<'b>(bodies: impl IntoIterator<Item = &'b str>) -> String {
    let mut out = String::new();
    for body in bodies {
        out.push('(');
        out.push_str(body);
        out.push_str(");\n");
    }
    out
}

/// The joined text split back into its bodies: each top-level statement's
/// expression, its parentheses dropped. `n` is the body count the text
/// must hold — anything else is an error, never a guess.
pub fn split_bodies(code: &str, n: usize) -> Result<Vec<String>, String> {
    use oxc_ast::ast::{Expression, Statement};
    use oxc_span::GetSpan;
    let allocator = oxc_allocator::Allocator::default();
    let ingest = crate::ingest::Ingest::parse(&allocator, code, "vendor-bodies.js");
    if let Some(e) = ingest.errors.first() {
        return Err(format!("the joined bodies do not parse: {e}"));
    }
    let mut out = Vec::with_capacity(n);
    for statement in &ingest.program.body {
        let Statement::ExpressionStatement(s) = statement else {
            return Err("a joined body is not an expression statement".into());
        };
        let mut e = &s.expression;
        while let Expression::ParenthesizedExpression(p) = e {
            e = &p.expression;
        }
        let span = e.span();
        out.push(code[span.start as usize..span.end as usize].to_string());
    }
    if out.len() != n {
        return Err(format!("{} bodies expected, {} found", n, out.len()));
    }
    Ok(out)
}

/// Name the bodies (names mode). `prior` is the prior release's
/// [`VENDOR_NAMING_SOURCE`] text. None when there is nothing to name.
///
/// Each body is formatted on its own (the formatter's strict parse can
/// refuse a sloppy-mode body; that body alone ships verbatim — its slot
/// in `texts` is its input text), then the formatted bodies are joined in
/// path order and named once. A named text that is not a valid pure
/// rename (the driver's verdict) or does not split back ships every body
/// verbatim.
pub fn name_vendor_bodies<P: NameProvider>(
    bodies: &[VendorBody],
    prior: Option<&str>,
    config: &NamingConfig,
    provider: &P,
    log: &mut DispatchLog,
) -> Result<Option<VendorNaming>, String> {
    let mut order: Vec<usize> = (0..bodies.len()).collect();
    order.sort_by(|&a, &b| bodies[a].path.cmp(&bodies[b].path));
    let mut formatted: Vec<(usize, String)> = Vec::new();
    let mut skipped: Vec<(String, Skip)> = Vec::new();
    for &i in &order {
        match format_body(&bodies[i].text) {
            Ok(text) => formatted.push((i, text)),
            Err(skip) => skipped.push((bodies[i].path.clone(), skip)),
        }
    }
    if formatted.is_empty() {
        return Ok(None);
    }
    let fresh: String = formatted.iter().map(|(_, t)| t.as_str()).collect();
    let holds: Vec<BTreeSet<String>> = formatted
        .iter()
        .map(|&(i, _)| bodies[i].restored.clone())
        .collect();
    let run = |prior: Option<&str>, log: &mut DispatchLog| {
        run_naming(
            &NamingInput {
                fresh: &fresh,
                prior,
                library: None,
                claimed_regions: &[],
                era_bridge: None,
                restorations: crate::naming::driver::no_restorations(),
                statement_holds: &holds,
            },
            config,
            provider,
            log,
        )
    };
    // A prior the driver refuses (the same-program check: it shares
    // nearly no code with these bodies) is dropped, never fatal.
    let (outcome, prior_used, prior_refusal) = match prior.map(|p| run(Some(p), log)) {
        Some(Ok(outcome)) => (outcome, true, None),
        Some(Err(why)) => (run(None, log)?, false, Some(why)),
        None => (run(None, log)?, false, None),
    };
    let mut texts: Vec<String> = bodies.iter().map(|b| b.text.clone()).collect();
    let named = outcome
        .code
        .as_deref()
        .filter(|_| outcome.output_valid)
        .and_then(|code| split_bodies(code, formatted.len()).ok());
    let joined = match named {
        Some(parts) => {
            for ((i, _), part) in formatted.iter().zip(parts) {
                texts[*i] = part;
            }
            outcome.code.clone().unwrap_or_default()
        }
        None => fresh.clone(),
    };
    Ok(Some(VendorNaming {
        texts,
        joined,
        prior_used,
        prior_refusal,
        skipped,
        outcome,
    }))
}

/// One body as the joined text holds it, or why it stays as extracted.
fn format_body(text: &str) -> Result<String, Skip> {
    if has_lone_surrogate_escape(text) {
        return Err(Skip::LoneSurrogate);
    }
    let formatted = crate::format::format_file(&join_bodies([text]), &Default::default(), &[])
        .map_err(|_| Skip::Unformattable)?
        .text;
    if formatted
        .lines()
        .any(|l| l.chars().count() > MAX_LINE_CHARS)
    {
        return Err(Skip::LongLine);
    }
    Ok(formatted)
}

/// A `\uXXXX` escape of a UTF-16 surrogate that is not one half of a
/// high-low pair. Conservative: an escaped backslash before `u` counts too.
fn has_lone_surrogate_escape(text: &str) -> bool {
    let bytes = text.as_bytes();
    let unit_at = |at: usize| -> Option<u16> {
        if bytes.get(at) != Some(&b'\\') || bytes.get(at + 1) != Some(&b'u') {
            return None;
        }
        let hex = text.get(at + 2..at + 6)?;
        u16::from_str_radix(hex, 16).ok()
    };
    let mut i = 0;
    while i + 6 <= bytes.len() {
        match unit_at(i) {
            Some(0xD800..=0xDBFF) => {
                if !matches!(unit_at(i + 6), Some(0xDC00..=0xDFFF)) {
                    return true;
                }
                i += 12;
            }
            Some(0xDC00..=0xDFFF) => return true,
            _ => i += 1,
        }
    }
    false
}

/// The vendored factory bodies the unpack wrote under `vendor/` (the
/// bridge's factory files; an app text asset under `src/` is not library
/// code and is left alone), each with the names Site B restored inside
/// its declaration — read back from `out_dir` as written.
pub fn bodies_of_bridge(
    out_dir: &std::path::Path,
    bridge: &crate::naming::module_names::EraBridge,
) -> Result<Vec<VendorBody>, String> {
    let mut out = Vec::new();
    for ((start, end), path) in &bridge.factories {
        if !path.starts_with("vendor/") {
            continue;
        }
        let file = out_dir.join(path);
        let text =
            std::fs::read_to_string(&file).map_err(|e| format!("read {}: {e}", file.display()))?;
        out.push(VendorBody {
            path: path.clone(),
            text,
            restored: bridge
                .restored_in((*start, *end))
                .map(str::to_string)
                .collect(),
        });
    }
    Ok(out)
}

/// The prior release's joined named bodies, when its tree has them.
pub fn prior_vendor_naming(prior_file: &std::path::Path) -> Option<String> {
    let root = crate::unpack::bun::find_prior_tree_root(prior_file)?;
    std::fs::read_to_string(root.join(VENDOR_NAMING_SOURCE)).ok()
}

/// Write the pass's result: every body whose text changed, and the joined
/// text the next release reads as its prior. Returns the files rewritten.
pub fn write_vendor_naming(
    out_dir: &std::path::Path,
    bodies: &[VendorBody],
    named: &VendorNaming,
) -> Result<usize, String> {
    let mut written = 0;
    for (body, text) in bodies.iter().zip(&named.texts) {
        if *text != body.text {
            let file = out_dir.join(&body.path);
            std::fs::write(&file, text).map_err(|e| format!("write {}: {e}", file.display()))?;
            written += 1;
        }
    }
    let joined = out_dir.join(VENDOR_NAMING_SOURCE);
    if let Some(parent) = joined.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("mkdir {}: {e}", parent.display()))?;
    }
    std::fs::write(&joined, &named.joined)
        .map_err(|e| format!("write {}: {e}", joined.display()))?;
    Ok(written)
}

#[cfg(test)]
mod vendor_bodies_test;
