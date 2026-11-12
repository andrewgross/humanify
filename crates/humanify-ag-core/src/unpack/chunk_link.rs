//! Stage 2's input side (exp216): a Bun code-split layout, linked back
//! into the ONE program every later stage reads.
//!
//! From Claude Code 2.1.242 (Bun 1.4), a compiled app ships as code-split
//! ESM: the entry is a small loader and the code lives in sibling chunks
//! that import each other by absolute bunfs specifiers —
//! `import{a as b}from"/$bunfs/root/chunk-<hash>.js"`, side-effect
//! imports, `import("/$bunfs/root/…")` and (from 2.1.250)
//! `import.meta.require("/$bunfs/root/…")`. The pipeline takes one file,
//! so it read only the loader and refused the prior ("only 1908 of 91971
//! prior functions have a matching structural hash").
//!
//! This is the `bun-chunks` layout of the input-layout registry
//! (`unpack::input`, whose `read_input` is the one reader of a run's input
//! file), chosen when [`is_chunked_entry`] holds; any other file takes the
//! `single-file` fallback and is returned byte-for-byte (every bundle
//! before 2.1.242, every e2e fixture but `bun-chunks`). This layout is linked into the
//! shape of the single-file Bun bundle that came before it, so detection,
//! the unpack adapter (bun) and everything after run unchanged:
//!
//! - the chunks reached from the entry (static imports, `import()`,
//!   `import.meta.require`, followed from the entry, never by listing the
//!   directory — chunk names are content hashes and the directory holds
//!   other files) are written into one `@bun-cjs` wrapper;
//! - every import of a chunk becomes a direct reference to the exporting
//!   chunk's binding (re-export chains followed); top-level names that
//!   would collide are renamed apart ([`names`]);
//! - module evaluation order is kept: the entry's static closure runs at
//!   the top level in ESM order; a chunk reached only lazily keeps its
//!   pure declarations at the top level (functions, and values that read
//!   nothing and do nothing — so Bun's `__commonJS` factories stay where
//!   the bun adapter finds them) and moves everything else into an init
//!   built with Bun's `__esm` shape, which runs once, after the inits of
//!   the lazy chunks it imports;
//! - `import(chunk)` becomes `Promise.resolve().then(() => (init(), ns))`
//!   — exactly what Bun writes for an `import()` within one file — and
//!   `import.meta.require(chunk)` becomes `(init(), ns)`, where `ns` is
//!   the chunk's namespace object in Bun's form (`var ns={};__export(ns,
//!   {name:()=>binding})`); other `import.meta` properties become their
//!   CommonJS bindings, and imports of Node built-ins take Bun's CommonJS
//!   shape (`ns=require(m)` read as `ns.name`; a default import as
//!   `.default` of `__toESM(require(m))`) — the shapes the prior release
//!   was matched in;
//! - the chunks are written in one depth-first walk from the entry
//!   ([`graph::Order`]), which keeps the eager chunks' ESM order and puts
//!   a lazily loaded chunk beside the chunk that loads it.
//!
//! What it refuses (an error, never a silent partial read): a bunfs
//! import whose file is missing, top-level await, `export *`, a bare or
//! unknown `import.meta`, an import of a name the chunk does not export.

mod analyse;
mod emit;
mod graph;
mod names;

use std::path::Path;

use analyse::{Chunk, Imported, Loader, Spec};

/// Bun's virtual filesystem root, as a compiled app's chunks name each
/// other.
const ROOT: &str = "/$bunfs/root/";

/// What the linker did — the stats file's `inputLink` block.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LinkReport {
    /// Files linked (the entry included).
    pub chunks_linked: usize,
    /// `.js` files beside the entry that no import reaches (not linked).
    pub js_files_unreached: usize,
    /// Chunks in the entry's static closure (run at load).
    pub eager: usize,
    /// Chunks reached only through a lazy load.
    pub lazy: usize,
    /// Lazy chunks given an init.
    pub inits: usize,
    /// `import(chunk)` sites.
    pub dynamic_imports: usize,
    /// `import.meta.require(chunk)` sites.
    pub sync_requires: usize,
    /// Top-level bindings renamed apart.
    pub renamed: usize,
}

impl LinkReport {
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "layout": super::input::InputLayout::BunChunks.name(),
            "chunksLinked": self.chunks_linked,
            "jsFilesUnreached": self.js_files_unreached,
            "eager": self.eager,
            "lazy": self.lazy,
            "inits": self.inits,
            "dynamicImports": self.dynamic_imports,
            "syncRequires": self.sync_requires,
            "renamed": self.renamed,
        })
    }
}

/// Is this file a Bun code-split entry: an ESM file under Bun's pragma
/// (`// @bun`, not the CommonJS `@bun-cjs`) that imports at least one
/// `/$bunfs/root/` sibling present beside it? The pragma and specifier
/// checks are text scans, so a single-file bundle is never parsed here.
pub fn is_chunked_entry(path: &Path, text: &str) -> bool {
    let first = text.lines().next().unwrap_or("");
    if !first.starts_with("// @bun") || first.contains("@bun-cjs") || !text.contains(ROOT) {
        return false;
    }
    let dir = path.parent().unwrap_or(Path::new("."));
    sibling_specifiers(text)
        .iter()
        .any(|rest| rest.ends_with(".js") && dir.join(rest).is_file())
}

/// Every `"/$bunfs/root/<rest>"` string in the text, as `<rest>`.
fn sibling_specifiers(text: &str) -> Vec<&str> {
    let needle = format!("\"{ROOT}");
    text.match_indices(&needle)
        .filter_map(|(i, _)| {
            let rest = &text[i + needle.len()..];
            rest.find('"').map(|end| &rest[..end])
        })
        .collect()
}

/// Link the layout whose entry is `entry` (see the module doc).
pub fn link(entry: &Path) -> Result<(String, LinkReport), String> {
    let mut loader = Loader::new(entry);
    let mut chunks: Vec<Chunk> = Vec::new();
    while let Some(path) = loader.queue.get(chunks.len()).cloned() {
        chunks.push(analyse::analyse(&path, &mut loader)?);
    }
    let order = graph::order(&chunks);
    let moved = graph::moved_statements(&chunks, &order);
    let needs_init = graph::needs_init(&chunks, &order, &moved);
    let needs_ns = namespaces_needed(&chunks);
    let names = names::assign(&chunks, &order, &needs_init, &needs_ns)?;
    let text = emit::program(&emit::Plan {
        chunks: &chunks,
        order: &order,
        moved: &moved,
        names: &names,
    })?;
    let report = LinkReport {
        chunks_linked: chunks.len(),
        js_files_unreached: unreached_js(&loader),
        eager: order.eager.iter().filter(|e| **e).count(),
        lazy: order.eager.iter().filter(|e| !**e).count(),
        inits: names.init.len(),
        dynamic_imports: chunks
            .iter()
            .flat_map(|c| &c.lazy)
            .filter(|e| !e.sync)
            .count(),
        sync_requires: chunks
            .iter()
            .flat_map(|c| &c.lazy)
            .filter(|e| e.sync)
            .count(),
        renamed: names.renamed,
    };
    Ok((text, report))
}

/// Chunks something loads lazily or imports as a namespace.
fn namespaces_needed(chunks: &[Chunk]) -> Vec<bool> {
    let mut out = vec![false; chunks.len()];
    for c in chunks {
        c.lazy.iter().for_each(|e| out[e.target] = true);
        for d in &c.imports {
            if let Spec::Chunk(t) = d.spec
                && d.specifiers
                    .iter()
                    .any(|(imp, _)| matches!(imp, Imported::Namespace))
            {
                out[t] = true;
            }
        }
    }
    out
}

fn unreached_js(loader: &Loader) -> usize {
    std::fs::read_dir(&loader.dir)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.is_file() && p.extension().is_some_and(|x| x == "js"))
                .filter(|p| !loader.index.contains_key(p))
                .count()
        })
        .unwrap_or(0)
}

#[cfg(test)]
mod chunk_link_test;
