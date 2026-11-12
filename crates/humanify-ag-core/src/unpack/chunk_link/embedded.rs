//! Bun's EMBEDDED FILES (exp217): the non-JS files a compiled app carries
//! beside its chunks (`/$bunfs/root/<name>`), and what the linked program
//! makes of each use. Read off Bun's source (oven-sh/bun, tags
//! bun-v1.4.1 / bun-v1.4.4 and main; experiments/217-bun-embedded-assets
//! has the per-loader table with file and line citations) and proved on
//! synthetic `bun build --compile --splitting` builds.
//!
//! What an importer gets, per loader (`src/bundler/ParseTask.rs`, the
//! per-loader arms of the parse):
//!
//! - `text`: from 1.4.1 an executable stores the text as a separate module
//!   and the import becomes `require("/$bunfs/root/<name>")` (an alias of
//!   `import.meta.require` in ESM chunks); the run-time loader answers it
//!   with the text as a string (`src/runtime/jsc_hooks.rs`,
//!   `is_text_module`). Linked, the read IS the text: a string literal,
//!   what Bun writes for a text import inside one file (1.4.0, and the
//!   single-file releases before the split);
//! - `napi` (`.node`): `require("/$bunfs/root/<name>")`, which Bun answers
//!   by writing the addon to a temp file and `dlopen`ing it. Linked, the
//!   `require` stays, of the tree's copy;
//! - `file` and `wasm`: the path string `"/$bunfs/root/<name>"`, which the
//!   app opens itself — Bun's fs (`readFile`, `exists`, `stat`, `access`,
//!   `readdir`; `src/runtime/node/node_fs.rs`), `Bun.file`, a `Worker`
//!   serve the embedded bytes under that path. Linked, the string becomes
//!   the tree's copy, `__dirname+"/embedded/<name>"` (the wrapper's
//!   `__dirname`, the tree root in a split tree), a real file every fs
//!   call reads as Bun would, and the file is listed for the finish stage
//!   to copy byte for byte ([`EmbeddedFile`];
//!   `finish::scaffold::write_embedded_files`);
//! - `sqlite_embedded`: `import.meta.require(name, {with: {type:
//!   "sqlite"}}).db` — refused (not linked; no Claude Code release has one);
//! - `json`, `jsonc`, `toml`, `yaml`, `json5`, `xml`, `md` (rendered to
//!   HTML), `dataurl`, `base64`: the value is written into the importing
//!   chunk at build time — never an embedded file, nothing to do.
//!
//! Only files the code names are taken, never the directory's listing.
//! A path string naming a module Bun built (a `.js` file under Bun's
//! pragma, e.g. a worker entry nothing imports) is code, not an embedded
//! file: it is left as is and counted. Refused, loudly: a name whose file
//! is missing, a name that leaves the root, a path string where
//! `__dirname` is rebound, a require with import attributes, text that
//! does not decode.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::{Component, Path, PathBuf};

use oxc_span::Span;
use oxc_syntax::symbol::SymbolId;

use super::analyse::{Chunk, Edit, StmtKind};
use super::graph::{Target, resolve_sym};

/// The output tree's folder for the embedded files a program reads by
/// path (beside `src/`, `vendor/` and `run.cjs`).
pub const EMBEDDED_DIR: &str = "embedded";

/// One embedded file the linked program reads by path.
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct EmbeddedFile {
    /// Its name under `/$bunfs/root/` — and under [`EMBEDDED_DIR`].
    pub rel: String,
    /// The unpacked file beside the entry.
    pub source: PathBuf,
}

/// The callee of a one-argument call whose argument is a bunfs string.
#[derive(Clone, Copy, Debug)]
pub(super) enum Callee {
    /// `import.meta.require(…)`.
    MetaRequire,
    /// A top-level binding (an alias of `import.meta.require`, or not).
    Sym(SymbolId),
}

/// A `"/$bunfs/root/<rest>"` string in expression position that is not a
/// chunk specifier.
#[derive(Clone, Debug)]
pub(super) struct FileRef {
    pub literal: Span,
    pub rest: String,
    /// The call whose first argument the string is.
    pub call: Option<(Span, Callee)>,
    /// That call passes more than the name (Bun's `sqlite_embedded`
    /// shape, `import.meta.require(name, {with: {type: "sqlite"}})`).
    pub extra_args: bool,
    /// No nested scope binds `__dirname` where the string sits.
    pub dirname_free: bool,
}

/// What the linked program makes of the chunks' embedded files.
#[derive(Default)]
pub(super) struct Resolved {
    /// Per chunk, the edits that replace each use.
    pub edits: Vec<Vec<Edit>>,
    pub files: Vec<EmbeddedFile>,
    pub text: usize,
    pub paths: usize,
    pub bun_module_paths: usize,
}

/// Is the file at `path` a module Bun built (a `.js` file whose first
/// line is Bun's pragma) — a chunk or an entry, not an embedded file?
pub(super) fn is_bun_module(path: &Path) -> bool {
    if path.extension().is_none_or(|x| x != "js") {
        return false;
    }
    let mut head = [0u8; 7];
    std::fs::File::open(path)
        .and_then(|mut f| f.read_exact(&mut head))
        .is_ok_and(|()| &head == b"// @bun")
}

/// The file `rest` names under the root `dir`, refusing a name that
/// leaves it.
fn file_of(dir: &Path, rest: &str) -> Result<PathBuf, String> {
    let inside = Path::new(rest)
        .components()
        .all(|c| matches!(c, Component::Normal(_)));
    if !inside || rest.contains('\\') {
        return Err(format!(
            "/$bunfs/root/{rest} leaves the embedded root; not linked"
        ));
    }
    let path = dir.join(rest);
    if !path.is_file() {
        return Err(format!(
            "/$bunfs/root/{rest} is read but {} does not exist",
            path.display()
        ));
    }
    Ok(path)
}

/// The text of an embedded text file as unpacked: Bun stores it as Latin-1
/// when every character is ASCII and as UTF-16LE otherwise
/// (`encode_text_module`, src/standalone_graph/StandaloneModuleGraph.rs;
/// the record's `encoding` byte, 1 or 2, which the unpacked file no
/// longer carries). A
/// file with a NUL is UTF-16 (ASCII text stored as UTF-16 has one in every
/// code unit); one without is UTF-8 when it decodes as UTF-8 (ASCII, or a
/// file an unpacker already decoded), else UTF-16 (text with no ASCII
/// character, e.g. pure CJK).
pub(super) fn decode_text(bytes: &[u8]) -> Result<String, String> {
    if !bytes.contains(&0)
        && let Ok(s) = std::str::from_utf8(bytes)
    {
        return Ok(s.to_string());
    }
    if !bytes.len().is_multiple_of(2) {
        return Err("is neither UTF-8 nor UTF-16LE (odd length)".into());
    }
    let units = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|p| u16::from_le_bytes(*p));
    char::decode_utf16(units)
        .collect::<Result<String, _>>()
        .map_err(|e| format!("is neither UTF-8 nor UTF-16LE ({e})"))
}

/// `text` as a JavaScript string literal, the way Bun's printer writes a
/// text import: the quote that needs the fewest escapes (`"`, then `'`,
/// then a template literal on a tie), raw newlines only in a template,
/// every character outside printable ASCII escaped.
pub(super) fn js_string(text: &str) -> String {
    let count = |c: char| text.chars().filter(|x| *x == c).count();
    let newlines = count('\n');
    let costs = [
        ('"', count('"') + newlines),
        ('\'', count('\'') + newlines),
        ('`', count('`') + text.matches("${").count()),
    ];
    let quote = costs
        .iter()
        .min_by_key(|(_, cost)| *cost)
        .map_or('"', |(q, _)| *q);
    let mut out = String::with_capacity(text.len() + 2);
    out.push(quote);
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' if quote == '`' => out.push('\n'),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '$' if quote == '`' && chars.peek() == Some(&'{') => out.push_str("\\$"),
            c if c == quote => {
                out.push('\\');
                out.push(c);
            }
            ' '..='~' => out.push(c),
            c if (c as u32) < 0x100 => out.push_str(&format!("\\x{:02X}", c as u32)),
            c => {
                let mut units = [0u16; 2];
                for u in c.encode_utf16(&mut units) {
                    out.push_str(&format!("\\u{u:04X}"));
                }
            }
        }
    }
    out.push(quote);
    out
}

/// Every `var x = import.meta.require` binding, in every chunk.
fn require_aliases(chunks: &[Chunk]) -> BTreeSet<Target> {
    let mut out = BTreeSet::new();
    for (c, chunk) in chunks.iter().enumerate() {
        for st in &chunk.stmts {
            if let StmtKind::Var(ds) = &st.kind {
                ds.iter()
                    .filter(|d| d.require_alias)
                    .flat_map(|d| d.names.iter())
                    .for_each(|s| {
                        out.insert(Target::Sym(c, *s));
                    });
            }
        }
    }
    out
}

/// Does this call read its argument the way Bun's `require` does?
fn is_require(
    chunks: &[Chunk],
    c: usize,
    callee: Callee,
    aliases: &BTreeSet<Target>,
) -> Result<bool, String> {
    Ok(match callee {
        Callee::MetaRequire => true,
        Callee::Sym(s) => aliases.contains(&resolve_sym(chunks, c, s)?),
    })
}

/// Decide every chunk's embedded-file uses (see the module doc).
pub(super) fn resolve(chunks: &[Chunk], dir: &Path) -> Result<Resolved, String> {
    let aliases = require_aliases(chunks);
    let mut out = Resolved::default();
    let mut files: BTreeMap<String, PathBuf> = BTreeMap::new();
    for (c, chunk) in chunks.iter().enumerate() {
        let mut edits = Vec::new();
        for r in &chunk.file_refs {
            let path = file_of(dir, &r.rest)?;
            let required = match r.call {
                Some((_, callee)) => is_require(chunks, c, callee, &aliases)?,
                None => false,
            };
            if required && r.extra_args {
                return Err(format!(
                    "{}: /$bunfs/root/{} is required with import attributes (Bun's \
                     sqlite_embedded loader); not linked",
                    chunk.path.display(),
                    r.rest
                ));
            }
            if is_bun_module(&path) {
                out.bun_module_paths += usize::from(!required);
                continue;
            }
            if let Some((call, _)) = r.call.filter(|_| required && !r.rest.ends_with(".node")) {
                let bytes = std::fs::read(&path)
                    .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
                let text =
                    decode_text(&bytes).map_err(|e| format!("/$bunfs/root/{} {e}", r.rest))?;
                edits.push(Edit {
                    start: call.start as usize,
                    end: call.end as usize,
                    text: js_string(&text),
                });
                out.text += 1;
                continue;
            }
            if !r.dirname_free {
                return Err(format!(
                    "{}: /$bunfs/root/{} sits where `__dirname` is bound; not linked",
                    chunk.path.display(),
                    r.rest
                ));
            }
            let at = serde_json::to_string(&format!("/{EMBEDDED_DIR}/{}", r.rest))
                .map_err(|e| e.to_string())?;
            edits.push(Edit {
                start: r.literal.start as usize,
                end: r.literal.end as usize,
                text: format!("__dirname+{at}"),
            });
            out.paths += 1;
            files.insert(r.rest.clone(), path);
        }
        out.edits.push(edits);
    }
    out.files = files
        .into_iter()
        .map(|(rel, source)| EmbeddedFile { rel, source })
        .collect();
    Ok(out)
}
