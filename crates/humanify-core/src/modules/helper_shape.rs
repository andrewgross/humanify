//! The CommonJS module helper, recognised by its SHAPE — the one place
//! that reads it out of a bundle's text without its name.
//!
//! Every Bun and esbuild build declares one helper that turns a module's
//! factory into a lazy `require`:
//!
//! ```text
//! Bun, esbuild <= 0.27 --minify   N=(A,B)=>()=>(B||A((B={exports:{}}).exports,B),B.exports)
//! esbuild >= 0.28 --minify        N=(A,B)=>()=>{try{return B||A((B={exports:{}}).exports,B),B.exports}catch(o){throw B=0,o}}
//! esbuild, unminified             __commonJS = (cb, mod) => function __require() {
//!                                   return mod || (0, cb[__getOwnPropNames(cb)[0]])((mod = { exports: {} }).exports, mod), mod.exports;
//! a re-minified esbuild build     N=(A,B)=>function(){return B||(0,A[K(A)[0]])((B={exports:{}}).exports,B),B.exports}
//! ```
//!
//! Minification renames the helper (`__commonJS` → `ee`) and may join it
//! into one `var` with every other helper; `--minify-identifiers` alone
//! keeps the spaced `{ exports: {} }`. What survives every form is the
//! core `(B = {exports: {}}).exports, B)` inside a two-parameter arrow
//! declared as `NAME = (A, B) =>`. [`find_module_helper`] reads exactly
//! that, whitespace-tolerant, and names the DECLARATOR whose initializer
//! holds it — not the first declarator of its statement (the frozen
//! marker scan's leftmost rule, which names `cye` for wrangler's
//! re-minified `var cye=Object.create,…,ee=(e,t)=>function(){…}`).
//!
//! The form also says which bundler wrote it, as far as text can
//! ([`ModuleHelperShape::esbuild_only`]): Bun's runtime never writes the
//! `try` guard (esbuild 0.28) or the `(0, cb[…])` indirect call
//! (esbuild's unminified runtime); the plain arrow is written by both.

use super::IdentifiedHelper;

/// How far before the marker the helper's `NAME = (A, B) =>` head may
/// start: the longest form (unminified esbuild's, with `try`) puts ~120
/// bytes between them; formatted text indents a little more.
const HEAD_WINDOW: usize = 512;

/// The module helper and the form it was written in.
pub struct ModuleHelperShape {
    pub helper: IdentifiedHelper,
    /// The thunk guards the factory call with `try` (esbuild >= 0.28).
    pub guarded: bool,
    /// The factory is called indirectly, `(0, A[…])(…)` — esbuild's
    /// unminified runtime (and a re-minified copy of it).
    pub indirect: bool,
    /// Byte offset of the marker's `{` — inside the helper's declaration.
    pub marker: usize,
}

impl ModuleHelperShape {
    /// A form only esbuild's runtime writes (Bun's runtime is the plain
    /// arrow, which esbuild <= 0.27 also writes when minifying).
    pub fn esbuild_only(&self) -> bool {
        self.guarded || self.indirect
    }
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$' || b >= 0x80
}

fn is_ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\x0b' | b'\x0c' | b'\r')
}

/// Skip whitespace forward from `i`.
fn skip_ws(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() && is_ws(bytes[i]) {
        i += 1;
    }
    i
}

/// Skip whitespace BACKWARD: the index just past the last non-space byte
/// before `end`.
fn skip_ws_back(bytes: &[u8], mut end: usize) -> usize {
    while end > 0 && is_ws(bytes[end - 1]) {
        end -= 1;
    }
    end
}

/// `pat` at `i` (after whitespace): the index past it.
fn eat(bytes: &[u8], i: usize, pat: &[u8]) -> Option<usize> {
    let i = skip_ws(bytes, i);
    bytes.get(i..i + pat.len()).filter(|s| *s == pat)?;
    Some(i + pat.len())
}

/// `pat` ending at `end` (before whitespace): the index where it starts.
fn eat_back(bytes: &[u8], end: usize, pat: &[u8]) -> Option<usize> {
    let end = skip_ws_back(bytes, end);
    let start = end.checked_sub(pat.len())?;
    (&bytes[start..end] == pat).then_some(start)
}

/// An identifier starting at `i` (after whitespace): (start, end).
fn ident_at(bytes: &[u8], i: usize) -> Option<(usize, usize)> {
    let start = skip_ws(bytes, i);
    let mut end = start;
    while end < bytes.len() && is_ident_byte(bytes[end]) {
        end += 1;
    }
    (end > start && !bytes[start].is_ascii_digit()).then_some((start, end))
}

/// An identifier ending at `end` (before whitespace): (start, end).
fn ident_back(bytes: &[u8], end: usize) -> Option<(usize, usize)> {
    let end = skip_ws_back(bytes, end);
    let mut start = end;
    while start > 0 && is_ident_byte(bytes[start - 1]) {
        start -= 1;
    }
    (start < end && !bytes[start].is_ascii_digit()).then_some((start, end))
}

/// The marker object `{ exports : { } }` starting at the `{` at `open`:
/// the index past its closing brace.
fn marker_end(bytes: &[u8], open: usize) -> Option<usize> {
    let i = eat(bytes, open, b"{")?;
    let i = eat(bytes, i, b"exports")?;
    let i = eat(bytes, i, b":")?;
    let i = eat(bytes, i, b"{")?;
    let i = eat(bytes, i, b"}")?;
    eat(bytes, i, b"}")
}

/// The CORE around one marker: `(B = {exports:{}}).exports, B)` — B's
/// name, the index of the `(` that opens the group, and whether the call
/// that receives it is indirect. `open` is the marker's `{`.
fn marker_core(source: &str, open: usize) -> Option<(&str, usize, bool)> {
    let bytes = source.as_bytes();
    let after = marker_end(bytes, open)?;
    let eq = eat_back(bytes, open, b"=")?;
    let (b_start, b_end) = ident_back(bytes, eq)?;
    let res = &source[b_start..b_end];
    let group_open = eat_back(bytes, b_start, b"(")?;
    // `(B={exports:{}}).exports,B)`
    let i = eat(bytes, after, b")")?;
    let i = eat(bytes, i, b".")?;
    let i = eat(bytes, i, b"exports")?;
    let i = eat(bytes, i, b",")?;
    let (r_start, r_end) = ident_at(bytes, i)?;
    if &source[r_start..r_end] != res {
        return None;
    }
    eat(bytes, r_end, b")")?;
    // The call receiving the group: `A(` (direct) or `(0, A[…])(`.
    let call_open = eat_back(bytes, group_open, b"(")?;
    let indirect = eat_back(bytes, call_open, b")").is_some();
    Some((res, call_open, indirect))
}

/// A helper head `NAME = (P1, P2) =>` whose `=>` starts at `arrow`: the
/// declaration's start (the `var`/`let`/`const` keyword or the joining
/// comma), the name, and the two parameters.
fn head_before_arrow(source: &str, arrow: usize) -> Option<(usize, &str, [&str; 2])> {
    let bytes = source.as_bytes();
    let close = eat_back(bytes, arrow, b")")?;
    let (p2s, p2e) = ident_back(bytes, close)?;
    let comma = eat_back(bytes, p2s, b",")?;
    let (p1s, p1e) = ident_back(bytes, comma)?;
    let open = eat_back(bytes, p1s, b"(")?;
    let eq = eat_back(bytes, open, b"=")?;
    // `==`, `!=`, `>=`, `+=`… are not a declarator's `=`.
    if eq > 0 && b"=!<>+-*/%&|^?".contains(&bytes[eq - 1]) {
        return None;
    }
    let (ns, ne) = ident_back(bytes, eq)?;
    let decl_start = if let Some(c) = eat_back(bytes, ns, b",") {
        c
    } else {
        // `var NAME` / `let NAME` / `const NAME`: at least one space, and
        // the keyword word-bounded on its left.
        let kw_end = skip_ws_back(bytes, ns);
        if kw_end == ns {
            return None;
        }
        let (ks, ke) = ident_back(bytes, kw_end)?;
        if !matches!(&source[ks..ke], "var" | "let" | "const") {
            return None;
        }
        ks
    };
    Some((
        decl_start,
        &source[ns..ne],
        [&source[p1s..p1e], &source[p2s..p2e]],
    ))
}

/// The helper declared around the marker at `open`, if this marker is a
/// module helper's core.
fn helper_around(source: &str, open: usize) -> Option<ModuleHelperShape> {
    let bytes = source.as_bytes();
    let (res, call_open, indirect) = marker_core(source, open)?;
    let mut window = open.saturating_sub(HEAD_WINDOW);
    while !source.is_char_boundary(window) {
        window += 1;
    }
    // The nearest head first: a thunk's own `() =>` fails the head parse
    // (no parameters) and the scan moves to the arrow before it.
    let segment = &source[window..open];
    for (rel, _) in segment.rmatch_indices("=>") {
        let arrow = window + rel;
        // A `;` between the head and the marker ends any helper: the
        // marker belongs to a later statement.
        if bytes[arrow..open].contains(&b';') {
            return None;
        }
        let Some((decl_start, name, params)) = head_before_arrow(source, arrow) else {
            continue;
        };
        if !params.contains(&res) {
            return None;
        }
        let body = &source[arrow..call_open];
        let guarded = body
            .match_indices("try")
            .any(|(i, _)| word_bounded(body.as_bytes(), i, 3));
        return Some(ModuleHelperShape {
            helper: IdentifiedHelper {
                name: name.to_string(),
                start_offset: decl_start as u32,
            },
            guarded,
            indirect,
            marker: open,
        });
    }
    None
}

fn word_bounded(bytes: &[u8], at: usize, len: usize) -> bool {
    (at == 0 || !is_ident_byte(bytes[at - 1]))
        && bytes.get(at + len).is_none_or(|b| !is_ident_byte(*b))
}

/// Whose `__export` registrar a descriptor is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ExportRegistrar {
    /// `{ get: A[B], enumerable: !0 }` (or `true`) that CLOSES there.
    Esbuild,
    /// The same getter going on with `configurable: !0, set: …` — Bun's
    /// runtime, minified or not.
    Bun,
}

/// The FIRST `__export` registrar in `source`, by shape: a property
/// descriptor `{ get: A[B], enumerable: !0 …`. esbuild's closes after
/// `enumerable`; Bun's always goes on with `configurable: !0, set: …`;
/// esbuild's other getters (`__copyProps`'s `get: () => from[key]`) are
/// arrows and are neither. The first one is the outer runtime's: a
/// bundle's own helpers precede the code it bundles, which may carry a
/// registrar of its own.
pub fn first_export_registrar(source: &str) -> Option<ExportRegistrar> {
    let bytes = source.as_bytes();
    source.match_indices("enumerable").find_map(|(at, _)| {
        let head = || -> Option<()> {
            let comma = eat_back(bytes, at, b",")?;
            let close = eat_back(bytes, comma, b"]")?;
            let (ks, _) = ident_back(bytes, close)?;
            let open = eat_back(bytes, ks, b"[")?;
            let (os, _) = ident_back(bytes, open)?;
            let colon = eat_back(bytes, os, b":")?;
            let get = eat_back(bytes, colon, b"get")?;
            eat_back(bytes, get, b"{")?;
            Some(())
        };
        let truthy = |i: usize| eat(bytes, i, b"!0").or_else(|| eat(bytes, i, b"true"));
        let tail = || -> Option<ExportRegistrar> {
            let i = eat(bytes, at + "enumerable".len(), b":")?;
            let i = truthy(i)?;
            if eat(bytes, i, b"}").is_some() {
                return Some(ExportRegistrar::Esbuild);
            }
            let i = eat(bytes, i, b",")?;
            let i = eat(bytes, i, b"configurable")?;
            let i = eat(bytes, i, b":")?;
            let i = eat(bytes, truthy(i)?, b",")?;
            let i = eat(bytes, i, b"set")?;
            eat(bytes, i, b":")?;
            Some(ExportRegistrar::Bun)
        };
        if !word_bounded(bytes, at, "enumerable".len()) {
            return None;
        }
        head()?;
        tail()
    })
}

/// The bundle's RUNTIME PROLOGUE: the text before the module helper first
/// wraps a module (`= NAME(` with a factory: `function`, an arrow, or
/// esbuild's keyed `{…}`). What follows is module bodies — code the bundle
/// carries, which may have been built by another bundler, or ship one as
/// a string — so a bundler's runtime is read from the prologue only. The
/// whole text when there is no helper or it is never called.
pub fn runtime_prologue(source: &str) -> &str {
    let Some(helper) = find_module_helper(source) else {
        return source;
    };
    let end = first_wrapper_call(source, &helper.helper.name, helper.marker);
    &source[..end.unwrap_or(source.len())]
}

/// The offset of `NAME` in the first `= NAME(factory` after `from`.
fn first_wrapper_call(source: &str, name: &str, from: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    source[from..].match_indices(name).find_map(|(rel, _)| {
        let at = from + rel;
        if !word_bounded(bytes, at, name.len()) {
            return None;
        }
        let eq = eat_back(bytes, at, b"=")?;
        if eq > 0 && b"=!<>+-*/%&|^?".contains(&bytes[eq - 1]) {
            return None;
        }
        let i = eat(bytes, at + name.len(), b"(")?;
        let factory = eat(bytes, i, b"function").is_some()
            || eat(bytes, i, b"(").is_some()
            || eat(bytes, i, b"{").is_some()
            || ident_at(bytes, i).is_some_and(|(_, e)| eat(bytes, e, b"=>").is_some());
        factory.then_some(at)
    })
}

/// The module helper of a Bun or esbuild bundle, by shape: the FIRST
/// marker (`{ exports: {} }`, any spacing) that sits in a helper's core,
/// and the declarator that holds it. None when no marker does.
pub fn find_module_helper(source: &str) -> Option<ModuleHelperShape> {
    let bytes = source.as_bytes();
    source.match_indices("exports").find_map(|(at, _)| {
        let open = eat_back(bytes, at, b"{")?;
        helper_around(source, open)
    })
}

#[cfg(test)]
mod helper_shape_test;
