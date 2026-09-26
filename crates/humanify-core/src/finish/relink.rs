//! Bun factory module re-linking for the runnable split — TS
//! `src/split/bun-relink.ts`.
//!
//! The Bun unpack writes each extracted `__commonJS` factory to `vendor/`
//! as its RAW factory expression and rewrites every reference to a FREE
//! identifier (`lib_234a1f83`); nothing binds those. This pass re-binds
//! them into an executable CommonJS graph:
//!
//! - every extracted factory file becomes a module exporting the memoizing
//!   thunk on a stable `exports.f` (`__commonJS(<factory>)`);
//! - every file that references a factory (split files AND other factory
//!   bodies) gets `const <id> = require("<rel>")` after its directive
//!   prologue, and each reference reads the thunk live (`<id>.f`), which is
//!   what makes the graph survive require cycles;
//! - `.humanify/__bun-runtime.js` provides `__commonJS` / `__esm`.
//!
//! Every edit is a byte splice of the ONE parse's reference positions —
//! untouched code stays byte-exact. Which identifiers are references is
//! Babel's question (`isReferencedIdentifier` + `scope.getBinding`), so it
//! is answered by the Babel scope view (WP3.1), never oxc's resolution.

use std::collections::BTreeMap;

use oxc_allocator::Allocator;
use oxc_ast::AstKind;

use humanify_model::js::{cmp_utf16, trim};

use crate::emit::paths::compute_relative_import_path;
use crate::graph::is_babel_assignment_target;
use crate::ingest::Ingest;
use crate::place::layout::METADATA_DIR;
use crate::rename::validated::RenameState;
use crate::trail::Anchor;
use crate::unpack::bun::{TO_COMMON_JS, TO_ESM};

/// The shared factory-helper runtime's path (a generated shim that lives
/// with the metadata, like `_bundle.js`).
pub fn bun_relink_runtime_filename() -> String {
    format!("{METADATA_DIR}/__bun-runtime.js")
}

/// The shared Bun factory helpers (the bundle's own `Q` / `__esm`), plus
/// Bun's `__toESM` / `__toCommonJS` interop helpers, which a vendored body
/// names when its factory called the bundle's (finding #51; the unpack
/// rewrites those references — `unpack::bun` scope planning).
pub const BUN_RELINK_RUNTIME: &str =
    "// Bun CJS/ESM factory helpers, extracted for the runnable split graph.
// __commonJS wraps a (exports, module) factory into a lazy, run-once,
// memoized thunk; __esm does the same for an ESM init function.
const __commonJS = (factory) => {
  let mod;
  return () => (
    mod || factory((mod = { exports: {} }).exports, mod), mod.exports
  );
};
const __esm = (factory) => {
  let value;
  return () => (factory && (value = factory((factory = 0))), value);
};
// Bun's module interop, as the bundle defines it: __toESM views a CommonJS
// exports object as an ES namespace, __toCommonJS the reverse.
const __accessProp = function (key) {
  return this[key];
};
const __toESMCache_node = new WeakMap();
const __toESMCache_esm = new WeakMap();
const __toESM = (mod, isNodeMode, target) => {
  const canCache = mod != null && typeof mod === \"object\";
  if (canCache) {
    const cached = (isNodeMode ? __toESMCache_node : __toESMCache_esm).get(mod);
    if (cached) return cached;
  }
  target = mod != null ? Object.create(Object.getPrototypeOf(mod)) : {};
  const to =
    isNodeMode || !mod || !mod.__esModule
      ? Object.defineProperty(target, \"default\", { value: mod, enumerable: true })
      : target;
  for (const key of Object.getOwnPropertyNames(mod))
    if (!Object.prototype.hasOwnProperty.call(to, key))
      Object.defineProperty(to, key, { get: __accessProp.bind(mod, key), enumerable: true });
  if (canCache) (isNodeMode ? __toESMCache_node : __toESMCache_esm).set(mod, to);
  return to;
};
const __moduleCache = new WeakMap();
const __toCommonJS = (from) => {
  let entry = __moduleCache.get(from);
  if (entry) return entry;
  entry = Object.defineProperty({}, \"__esModule\", { value: true });
  if ((from && typeof from === \"object\") || typeof from === \"function\")
    for (const key of Object.getOwnPropertyNames(from))
      if (!Object.prototype.hasOwnProperty.call(entry, key)) {
        const desc = Object.getOwnPropertyDescriptor(from, key);
        Object.defineProperty(entry, key, {
          get: __accessProp.bind(from, key),
          enumerable: !desc || desc.enumerable,
        });
      }
  __moduleCache.set(from, entry);
  return entry;
};
module.exports = { __commonJS, __esm, __toESM, __toCommonJS };
";

/// The runtime helpers a vendored body may name free, bound from the shim.
const INTEROP_HELPERS: [&str; 2] = [TO_ESM, TO_COMMON_JS];

/// runtimeIdentifier → the file that defines that factory
/// (`FactoryLookup`; only `has` / `get` are read, so the map's order is
/// unobservable).
pub type FactoryLookup = BTreeMap<String, String>;

/// The property holding the memoizing thunk (`THUNK_PROP`).
const THUNK_PROP: &str = "f";

/// A Babel parse of standalone text (`parseFileAst` → `parseSync`, source
/// type `unambiguous`), or the parse error (`parseSync` THROWS).
pub(crate) fn parse_or_err<'a>(
    allocator: &'a Allocator,
    code: &'a str,
) -> Result<Ingest<'a>, String> {
    let ingest = Ingest::parse_unambiguous(allocator, code);
    match ingest.errors.first() {
        Some(e) => Err(e.clone()),
        None => Ok(ingest),
    }
}

/// One free reference to a factory id.
struct FactoryRef {
    name: String,
    /// Byte offset just after the identifier.
    end: usize,
    /// The value of a shorthand object property (`({lib_x})`): the splice
    /// must expand it (`lib_x: lib_x.f`), since `({lib_x.f})` is a syntax
    /// error (finding #29).
    shorthand: bool,
}

/// Is this reference the VALUE of a shorthand object-literal property?
/// (Shorthand assignment targets are constant violations, never here.)
fn is_shorthand_property_value(
    nodes: &oxc_semantic::AstNodes<'_>,
    id: oxc_semantic::NodeId,
) -> bool {
    let parent = nodes.parent_id(id);
    parent != id && matches!(nodes.kind(parent), AstKind::ObjectProperty(p) if p.shorthand)
}

/// The free (unbound) Babel references whose name is a known factory id.
fn factory_refs(ingest: &Ingest<'_>, lookup: &FactoryLookup) -> Vec<FactoryRef> {
    let semantic = ingest.semantic();
    let nodes = semantic.nodes();
    let state = RenameState::new(semantic, Anchor::Generated);
    let mut refs = Vec::new();
    for node in nodes.iter() {
        let AstKind::IdentifierReference(ident) = node.kind() else {
            continue;
        };
        let name = ident.name.as_str();
        if !lookup.contains_key(name) {
            continue;
        }
        // isReferencedIdentifier: an assignment target is a constant
        // violation in Babel, never a reference.
        if is_babel_assignment_target(nodes, node.id()) {
            continue;
        }
        if state
            .get_binding(state.view().scope_of_node(node.id()), name)
            .is_some()
        {
            continue; // shadowed by a local binding
        }
        refs.push(FactoryRef {
            name: name.to_string(),
            end: ident.span.end as usize,
            shorthand: is_shorthand_property_value(nodes, node.id()),
        });
    }
    refs
}

/// After the directive prologue, else the first statement, else the end
/// (`headerInsertOffset`, computed on the pre-splice parse).
fn header_insert_offset(ingest: &Ingest<'_>, code: &str) -> usize {
    let program = ingest.program;
    if let Some(last) = program.directives.last() {
        return last.span.end as usize;
    }
    if let Some(first) = program.body.first() {
        return oxc_span::GetSpan::span(first).start as usize;
    }
    code.len()
}

fn insert_header_at(code: &str, at: usize, lines: &[String]) -> String {
    let block = lines.join("\n");
    if at == 0 {
        return format!("{block}\n{code}");
    }
    format!("{}\n{block}{}", &code[..at], &code[at..])
}

/// `relinkFactoryReferences`: inject the require headers and rewrite each
/// reference `<id>` → `<id>.f`. One parse per file.
pub fn relink_factory_references(
    code: &str,
    from_file: &str,
    lookup: &FactoryLookup,
) -> Result<String, String> {
    let allocator = Allocator::default();
    let ingest = parse_or_err(&allocator, code)?;
    let mut refs = factory_refs(&ingest, lookup);
    if refs.is_empty() {
        return Ok(code.to_string());
    }
    let at = header_insert_offset(&ingest, code);
    // Splice right-to-left so earlier offsets stay valid (a stable sort by
    // descending end, as the TS's `sort((a, b) => b.end - a.end)`).
    refs.sort_by_key(|r| std::cmp::Reverse(r.end));
    let mut spliced = code.to_string();
    for r in &refs {
        let splice = if r.shorthand {
            format!(": {}.{THUNK_PROP}", r.name)
        } else {
            format!(".{THUNK_PROP}")
        };
        spliced.insert_str(r.end, &splice);
    }
    let mut ids: Vec<&str> = refs.iter().map(|r| r.name.as_str()).collect();
    ids.sort_by(|a, b| cmp_utf16(a, b));
    ids.dedup();
    let lines: Vec<String> = ids
        .iter()
        .map(|id| {
            let to = lookup.get(*id).map_or(*id, String::as_str);
            format!(
                "const {id} = require(\"{}\");",
                compute_relative_import_path(from_file, to)
            )
        })
        .collect();
    Ok(insert_header_at(&spliced, at, &lines))
}

/// The interop helpers `body` references FREE (unbound anywhere in it).
fn free_interop_helpers(body: &str) -> Result<Vec<&'static str>, String> {
    // The body is an EXPRESSION: parenthesized, so a `function (…) {…}`
    // factory is not read as a nameless declaration.
    let expression = format!("({body}\n)");
    let allocator = Allocator::default();
    let ingest = parse_or_err(&allocator, &expression)?;
    let unresolved = ingest.semantic().scoping().root_unresolved_references();
    Ok(INTEROP_HELPERS
        .into_iter()
        .filter(|h| unresolved.keys().any(|k| k == h))
        .collect())
}

/// `wrapExtractedFactory`: an extracted factory body (a raw
/// `(exports, module) => {…}` expression) as a runnable CJS module
/// exporting the memoizing thunk, its factory references re-bound and the
/// interop helpers it names bound from the shim.
pub fn wrap_extracted_factory(
    body: &str,
    from_file: &str,
    lookup: &FactoryLookup,
) -> Result<String, String> {
    let rt = compute_relative_import_path(from_file, &bun_relink_runtime_filename());
    let mut bound = vec!["__commonJS"];
    bound.extend(free_interop_helpers(trim(body))?);
    // `exports.f = …` MUTATES the initial exports object, so a cyclic
    // requirer's captured identity stays valid.
    let wrapped = format!(
        "const {{ {} }} = require(\"{rt}\");\nexports.{THUNK_PROP} = __commonJS({});\n",
        bound.join(", "),
        trim(body)
    );
    relink_factory_references(&wrapped, from_file, lookup)
}

#[cfg(test)]
mod relink_test;
