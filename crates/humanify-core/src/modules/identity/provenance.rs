//! What each BUNDLER kept about a module — the implementations behind the
//! toolchain's `ModuleIdentitySource` piece (P17). Nothing else in the
//! pipeline asks which bundler wrote a module's provenance; the readers
//! ([`super::readers`]) read the [`ModuleProvenance`] these return.
//!
//! Researched against Bun 1.3.14 / 1.4.x and esbuild 0.27.2 source and
//! real builds (/work/library-identity-2026-10-10/bun-layout.md §2, §4):
//!
//! - **Bun** keeps only `/*!` (and `//!`) legal comments, in place, next to
//!   the module (`lexer.zig`: `text[2] == '!'`); the classification reads
//!   the one before the factory statement or inside its body
//!   (`FactoryRecord::banner_text`, captured on the RAW bundle — the
//!   formatter drops comments). It inlines the build machine's absolute
//!   path wherever a module asked for its own location: `__dirname` /
//!   `__filename` (written `var __dirname="/abs/…"`), `require.resolve(…)`,
//!   and `import.meta.url` in CommonJS output (`"file:///…"`). No source
//!   path is printed when minified.
//! - **esbuild** keeps each module's source path as its wrapper's key
//!   (`__commonJS({"node_modules/ms/index.js"(exports, module){…}})`) until
//!   identifiers are minified (`FactoryRecord::source_path`), and by
//!   default moves every legal comment (`/*!`, `@license`, `@preserve`)
//!   into one end-of-file block keyed by `package/file`. It inlines no
//!   absolute paths.

use super::{BundleFacts, DeclaredPath, LegalComment, ModuleProvenance, PathKind};

/// The comment esbuild opens its end-of-file legal block with.
const ESBUILD_LEGAL_BLOCK: &str = "/*! Bundled license information:";

fn quoted_text(t: &str) -> Option<&str> {
    let q = t.chars().next()?;
    (matches!(q, '"' | '\'' | '`') && t.len() >= 2 && t.ends_with(q)).then(|| &t[1..t.len() - 1])
}

/// An absolute path as a bundler inlines it: POSIX, a `file://` URL, or a
/// Windows drive path.
fn is_absolute(p: &str) -> bool {
    p.starts_with('/')
        || p.starts_with("file:///")
        || (p.len() > 2 && p.as_bytes()[1] == b':' && matches!(p.as_bytes()[2], b'\\' | b'/'))
}

/// Bun: the kept legal comment, and the absolute paths Bun inlined for the
/// module's own location requests (`__dirname = "…"`, `__filename = "…"`,
/// `<require>.resolve("…")`, an inlined `import.meta.url`).
pub fn bun_module_provenance(body: &str, kept_comment: Option<&str>) -> ModuleProvenance {
    let t = crate::modules::vendor_pairing::lex_raw(body);
    let at = |i: usize| t.get(i).map_or("", String::as_str);
    let mut paths: Vec<DeclaredPath> = Vec::new();
    for (i, token) in t.iter().enumerate() {
        let Some(value) = quoted_text(token) else {
            continue;
        };
        let inlined_location =
            (i >= 2 && matches!(at(i - 2), "__dirname" | "__filename") && at(i - 1) == "=")
                || (i >= 3 && at(i - 3) == "." && at(i - 2) == "resolve" && at(i - 1) == "(")
                || value.starts_with("file:///");
        if inlined_location && is_absolute(value) && !paths.iter().any(|p| p.path == value) {
            paths.push(DeclaredPath {
                kind: PathKind::Inlined,
                path: value.to_string(),
            });
        }
    }
    ModuleProvenance {
        kept_comments: kept_comment.map(str::to_string).into_iter().collect(),
        paths,
    }
}

/// esbuild: the module's own source path (its wrapper key, when kept), the
/// legal comment kept in place (`--legal-comments=inline`), and the
/// end-of-file block's comments keyed by that source path.
pub fn esbuild_module_provenance(
    kept_comment: Option<&str>,
    source_path: Option<&str>,
    bundle: &BundleFacts,
) -> ModuleProvenance {
    let mut kept_comments: Vec<String> = kept_comment.map(str::to_string).into_iter().collect();
    if let Some(path) = source_path {
        let key = path.strip_prefix("node_modules/").unwrap_or(path);
        kept_comments.extend(
            bundle
                .legal_comments
                .iter()
                .filter(|c| c.path == key)
                .map(|c| c.text.clone()),
        );
    }
    ModuleProvenance {
        kept_comments,
        paths: source_path
            .map(|p| DeclaredPath {
                kind: PathKind::ModuleFile,
                path: p.to_string(),
            })
            .into_iter()
            .collect(),
    }
}

/// esbuild's end-of-file legal block:
///
/// ```text
/// /*! Bundled license information:
///
/// legal-lib/index.js:
///   (*! legal-lib v1.2.3 | MIT *)
///   (**
///    * @license …
///    *)
/// */
/// ```
///
/// One entry per `(* … *)` comment, keyed by the `package/file` line above
/// it. Empty when the bundle has no such block.
pub fn esbuild_bundle_facts(code: &str) -> BundleFacts {
    let Some(start) = code.rfind(ESBUILD_LEGAL_BLOCK) else {
        return BundleFacts::default();
    };
    let block = &code[start + ESBUILD_LEGAL_BLOCK.len()..];
    let block = &block[..block.find("*/").unwrap_or(block.len())];
    let mut legal_comments = Vec::new();
    let mut path: Option<&str> = None;
    let mut section = String::new();
    let flush = |path: Option<&str>, section: &str, out: &mut Vec<LegalComment>| {
        let Some(path) = path else {
            return;
        };
        let mut rest = section;
        while let Some(open) = rest.find("(*") {
            let after = &rest[open + 2..];
            let Some(close) = after.find("*)") else {
                break;
            };
            out.push(LegalComment {
                path: path.to_string(),
                text: after[..close].to_string(),
            });
            rest = &after[close + 2..];
        }
    };
    for line in block.lines() {
        let is_key = !line.starts_with([' ', '\t']) && line.trim_end().ends_with(':');
        if is_key {
            flush(path, &section, &mut legal_comments);
            path = Some(line.trim_end().trim_end_matches(':'));
            section.clear();
        } else {
            section.push_str(line);
            section.push('\n');
        }
    }
    flush(path, &section, &mut legal_comments);
    BundleFacts { legal_comments }
}
