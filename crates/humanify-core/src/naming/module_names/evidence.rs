//! What the module step shows the model about one module
//! (docs/design/module-naming.md, "What it shows per module"): its
//! declarations, what its setup code assigns, the object other code reads
//! it as, the modules it imports, the libraries it uses, its strings, and
//! a code excerpt — every name as it is NOW (after the waves).
//!
//! Masking: the module's own wrapper prints as `MODULE_INIT`, the lazy-init
//! helper as `__esm`, and other modules' wrappers never appear by name —
//! a bare `initOther();` statement is dropped (it is listed under
//! "Imports"), any other `initOther()` call prints as
//! `loadModule("<its first declaration>")`. Their names are minified while
//! the step runs, or a model's old answer that would invite an echo.
//!
//! Sizes are capped by COUNT (the 2026-10-05 sizing found one entry of
//! 23.7K characters — a schema module declaring ~200 names on one `var`
//! line): at most [`MAX_DECLARED`] declared names, [`MAX_ASSIGNS`] setup
//! assignments, [`MAX_IMPORTS`] imports, [`MAX_LIBRARIES`] libraries,
//! [`MAX_STRINGS`] strings, and a code excerpt of about
//! [`EXCERPT_CHARS`] characters.

use std::collections::{HashMap, HashSet};

use oxc_span::Span;
use serde_json::Value;

use crate::naming::waves::generate::{Replacement, TextView};
use crate::naming::waves::render::Occurrences;
use crate::rename::validated::RenameState;
use crate::rename::validated::scopes::BindingId;
use crate::twins::fossil::{FossilModule, is_export_registration};

/// Declared names shown (one per declarator, not per statement).
pub const MAX_DECLARED: usize = 14;
/// Setup-code assignments shown.
pub const MAX_ASSIGNS: usize = 10;
/// Imported modules shown.
pub const MAX_IMPORTS: usize = 8;
/// Names shown per imported module.
const NAMES_PER_IMPORT: usize = 3;
/// Libraries shown.
pub const MAX_LIBRARIES: usize = 6;
/// Distinct string literals shown.
pub const MAX_STRINGS: usize = 8;
/// A string literal's length bounds.
const STRING_CHARS: std::ops::RangeInclusive<usize> = 3..=60;
/// The code excerpt's budget (the wrapper body first, then every other
/// statement's first lines, deepened while they fit).
pub const EXCERPT_CHARS: usize = 1_600;
/// The wrapper body's share of the excerpt, and its line cap.
const HEAD_CHARS: usize = 700;
const HEAD_LINES: usize = 16;
/// A short constant's initializer is shown inline up to this length.
const INLINE_INIT_CHARS: usize = 40;
/// One declaration's header (a function's signature) is cut here.
const DECL_CHARS: usize = 100;
/// Lines of one statement ever rendered for the excerpt.
const STATEMENT_LINES: usize = 200;

/// What the evidence of every module reads: the parsed text, the CURRENT
/// names, and which bindings are wrappers.
pub struct EvidenceSource<'a, 's> {
    pub text: &'s str,
    pub view: &'a TextView<'s>,
    pub occ: &'a Occurrences,
    pub state: &'a RenameState,
    /// The wrapper body's top-level statements (the modules index them).
    pub body: &'a [Value],
    pub modules: &'a [FossilModule],
    /// Each module's wrapper binding (None: not resolved).
    pub wrappers: &'a [Option<BindingId>],
    /// The lazy-init helper's binding(s).
    pub helpers: &'a HashSet<BindingId>,
    /// Which module each wrapper binding belongs to.
    wrapper_of: HashMap<BindingId, usize>,
}

fn node_type(v: &Value) -> &str {
    v.get("type").and_then(Value::as_str).unwrap_or("")
}

fn span_of(v: &Value) -> Option<Span> {
    Some(Span::new(
        u32::try_from(v.get("start")?.as_u64()?).ok()?,
        u32::try_from(v.get("end")?.as_u64()?).ok()?,
    ))
}

/// A string cut to at most `max` characters on a char boundary.
fn cut(s: &str, max: usize) -> String {
    match s.char_indices().nth(max) {
        Some((i, _)) => format!("{}…", &s[..i]),
        None => s.to_string(),
    }
}

fn collapse_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

impl<'a, 's> EvidenceSource<'a, 's> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        text: &'s str,
        view: &'a TextView<'s>,
        occ: &'a Occurrences,
        state: &'a RenameState,
        body: &'a [Value],
        modules: &'a [FossilModule],
        wrappers: &'a [Option<BindingId>],
        helpers: &'a HashSet<BindingId>,
    ) -> Self {
        let wrapper_of = wrappers
            .iter()
            .enumerate()
            .filter_map(|(i, b)| b.map(|b| (b, i)))
            .collect();
        EvidenceSource {
            text,
            view,
            occ,
            state,
            body,
            modules,
            wrappers,
            helpers,
            wrapper_of,
        }
    }

    /// The current name of the identifier starting at `start` (its text
    /// when no binding owns it).
    fn current(&self, ident: &Value) -> Option<String> {
        let name = ident.get("name").and_then(Value::as_str)?;
        let start = span_of(ident)?.start;
        Some(
            self.occ
                .current_at(self.state, start)
                .unwrap_or(name)
                .to_string(),
        )
    }

    /// The identifiers a statement DECLARES (function / class / var
    /// declarator ids), as nodes.
    fn declared_idents(stmt: &Value) -> Vec<&Value> {
        match node_type(stmt) {
            "FunctionDeclaration" | "ClassDeclaration" => stmt
                .get("id")
                .filter(|id| node_type(id) == "Identifier")
                .into_iter()
                .collect(),
            "VariableDeclaration" => stmt
                .get("declarations")
                .and_then(Value::as_array)
                .map(|ds| {
                    ds.iter()
                        .filter_map(|d| d.get("id").filter(|id| node_type(id) == "Identifier"))
                        .collect()
                })
                .unwrap_or_default(),
            _ => Vec::new(),
        }
    }

    /// The bundler's own plumbing: a statement declaring the lazy-init
    /// helper (it sits in the first module's segment).
    fn is_plumbing(&self, stmt: &Value) -> bool {
        Self::declared_idents(stmt).iter().any(|id| {
            span_of(id)
                .and_then(|s| self.occ.binding_at(s.start))
                .is_some_and(|b| self.helpers.contains(&b))
        })
    }

    /// The module's statements the evidence reads (plumbing excluded),
    /// with or without its init definition.
    fn statements(&self, m: usize, with_init: bool) -> Vec<usize> {
        let module = &self.modules[m];
        module
            .statements
            .iter()
            .copied()
            .filter(|&i| with_init || i != module.init_index)
            .filter(|&i| !self.is_plumbing(&self.body[i]))
            .collect()
    }

    /// A module's own declared names, current, its wrapper excluded.
    pub fn own_declared(&self, m: usize) -> Vec<String> {
        let module = &self.modules[m];
        self.statements(m, true)
            .into_iter()
            .flat_map(|i| Self::declared_idents(&self.body[i]))
            .filter(|id| id.get("name").and_then(Value::as_str) != Some(&module.init_name))
            .filter_map(|id| self.current(id))
            .collect()
    }

    /// The edits that print `span` under the current names, with the
    /// wrappers and the helper masked.
    fn masked_edits(&self, m: usize, node: &Value, span: Span) -> Vec<Replacement> {
        let mut masks: Vec<Replacement> = Vec::new();
        self.collect_masks(m, node, &mut masks);
        masks.sort_by_key(|r| r.span.start);
        let mut edits: Vec<Replacement> = self
            .occ
            .edits(self.text, self.state, span)
            .into_iter()
            .filter(|e| {
                !masks
                    .iter()
                    .any(|m| e.span.start < m.span.end && m.span.start < e.span.end)
            })
            .collect();
        edits.extend(masks);
        edits.sort_by_key(|r| (r.span.start, r.span.end));
        edits
    }

    fn collect_masks(&self, m: usize, node: &Value, out: &mut Vec<Replacement>) {
        match node {
            Value::Array(items) => {
                for item in items {
                    self.collect_masks(m, item, out);
                }
            }
            Value::Object(map) => {
                if let Some(masked) = self.mask_of(m, node) {
                    out.push(masked);
                    return;
                }
                for (k, v) in map {
                    if k != "type" {
                        self.collect_masks(m, v, out);
                    }
                }
            }
            _ => {}
        }
    }

    /// The other module a zero-arg call `initOther()` loads.
    fn loaded_module(&self, call: &Value) -> Option<usize> {
        if node_type(call) != "CallExpression"
            || !call
                .get("arguments")
                .and_then(Value::as_array)
                .is_some_and(Vec::is_empty)
        {
            return None;
        }
        let callee = call
            .get("callee")
            .filter(|c| node_type(c) == "Identifier")?;
        let binding = self.occ.binding_at(span_of(callee)?.start)?;
        self.wrapper_of.get(&binding).copied()
    }

    fn mask_of(&self, m: usize, node: &Value) -> Option<Replacement> {
        let span = span_of(node)?;
        let replace = |text: String| Some(Replacement { span, text });
        match node_type(node) {
            "ExpressionStatement" => {
                let other = self.loaded_module(node.get("expression")?)?;
                (other != m).then_some(())?;
                replace(String::new())
            }
            "CallExpression" => {
                let other = self.loaded_module(node)?;
                (other != m).then_some(())?;
                let first = self.own_declared(other).into_iter().next();
                replace(format!(
                    "loadModule({})",
                    serde_json::to_string(first.as_deref().unwrap_or("?")).ok()?
                ))
            }
            "Identifier" => {
                let binding = self.occ.binding_at(span.start)?;
                if self.wrappers[m] == Some(binding) {
                    replace("MODULE_INIT".into())
                } else if self.helpers.contains(&binding) {
                    replace("__esm".into())
                } else if self.wrapper_of.contains_key(&binding) {
                    replace("loadModule".into())
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// One statement printed under the current names, masked, at most
    /// `lines` lines (whitespace-only lines a dropped statement left are
    /// removed).
    fn render(&self, m: usize, stmt: &Value, lines: usize) -> Vec<String> {
        let Some(span) = span_of(stmt) else {
            return Vec::new();
        };
        let edits = self.masked_edits(m, stmt, span);
        let (text, _) = self
            .view
            .pretty_lines(span, &edits, false, lines.saturating_sub(1));
        text.lines()
            .filter(|l| !l.trim().is_empty())
            .map(str::to_string)
            .collect()
    }

    /// `function name(params)` / `class Name extends X` / `var name = init`.
    fn declaration_lines(&self, m: usize, stmt: &Value) -> Vec<String> {
        match node_type(stmt) {
            "FunctionDeclaration" | "ClassDeclaration" => {
                let (Some(span), Some(body)) = (span_of(stmt), stmt.get("body").and_then(span_of))
                else {
                    return Vec::new();
                };
                let head = Span::new(span.start, body.start);
                let edits = self.masked_edits(m, stmt, head);
                let (text, _) = self.view.pretty_lines(head, &edits, false, usize::MAX);
                vec![cut(&collapse_ws(&text), DECL_CHARS)]
            }
            "VariableDeclaration" => stmt
                .get("declarations")
                .and_then(Value::as_array)
                .map(|ds| {
                    ds.iter()
                        .filter_map(|d| self.declarator_line(m, d))
                        .collect()
                })
                .unwrap_or_default(),
            _ => Vec::new(),
        }
    }

    fn declarator_line(&self, m: usize, d: &Value) -> Option<String> {
        let id = d.get("id").filter(|id| node_type(id) == "Identifier")?;
        let name = self.current(id)?;
        let inline = d.get("init").filter(|i| !i.is_null()).and_then(|init| {
            let span = span_of(init)?;
            let edits = self.masked_edits(m, init, span);
            let (text, more) = self.view.pretty_lines(span, &edits, false, 0);
            (!more && text.chars().count() <= INLINE_INIT_CHARS && !text.ends_with('{'))
                .then_some(text)
        });
        Some(match inline {
            Some(init) => format!("var {name} = {init}"),
            None => format!("var {name}"),
        })
    }

    /// The init function's body statements (the `__esm` argument: the
    /// function itself in Bun's form, the keyed method in esbuild's).
    fn init_body(stmt: &Value) -> &[Value] {
        fn find(v: &Value) -> Option<&Vec<Value>> {
            match v {
                Value::Object(map) => {
                    if matches!(
                        node_type(v),
                        "ArrowFunctionExpression" | "FunctionExpression"
                    ) && let Some(body) =
                        map.get("body").filter(|b| node_type(b) == "BlockStatement")
                    {
                        return body.get("body").and_then(Value::as_array);
                    }
                    map.values().find_map(find)
                }
                Value::Array(items) => items.iter().find_map(find),
                _ => None,
            }
        }
        find(stmt).map_or(&[], Vec::as_slice)
    }

    /// The module bindings the setup code assigns (`x = …` at the init
    /// body's top level, sequences included).
    fn assigns(&self, m: usize) -> Vec<String> {
        let init = &self.body[self.modules[m].init_index];
        let mut out: Vec<String> = Vec::new();
        let mut push = |e: &Value| {
            if node_type(e) == "AssignmentExpression"
                && e.get("operator").and_then(Value::as_str) == Some("=")
                && let Some(left) = e.get("left").filter(|l| node_type(l) == "Identifier")
                && let Some(name) = self.current(left)
                && !out.contains(&name)
            {
                out.push(name);
            }
        };
        for s in Self::init_body(init) {
            let Some(e) = s
                .get("expression")
                .filter(|_| node_type(s) == "ExpressionStatement")
            else {
                continue;
            };
            match node_type(e) {
                "SequenceExpression" => e
                    .get("expressions")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .for_each(&mut push),
                _ => push(e),
            }
        }
        out
    }

    /// The object other code reads the module as: the export registrar's
    /// target (`register(ns, { name: () => name })`).
    fn namespace(&self, m: usize) -> Option<String> {
        self.modules[m]
            .statements
            .iter()
            .map(|&i| &self.body[i])
            .find(|s| is_export_registration(s))
            .and_then(|s| {
                s.get("expression")?
                    .get("arguments")?
                    .as_array()?
                    .first()
                    .and_then(|t| self.current(t))
            })
    }

    /// Every string literal and `require` specifier of the module, in
    /// statement order.
    fn literals(&self, m: usize) -> (Vec<String>, Vec<String>) {
        fn walk(v: &Value, strings: &mut Vec<String>, requires: &mut Vec<String>) {
            match v {
                Value::Object(map) => {
                    if let Some(spec) = require_spec(v) {
                        if !requires.contains(&spec) {
                            requires.push(spec);
                        }
                        return;
                    }
                    if node_type(v) == "Literal"
                        && let Some(s) = map.get("value").and_then(Value::as_str)
                    {
                        let shown = STRING_CHARS.contains(&s.chars().count())
                            && !s.contains(['\n', '\r'])
                            && !(s.starts_with('.') && s.contains('/') && !s.contains(' '));
                        if shown && !strings.iter().any(|x| x == s) {
                            strings.push(s.to_string());
                        }
                    }
                    for (k, child) in map {
                        if k != "type" {
                            walk(child, strings, requires);
                        }
                    }
                }
                Value::Array(items) => items.iter().for_each(|i| walk(i, strings, requires)),
                _ => {}
            }
        }
        let (mut strings, mut requires) = (Vec::new(), Vec::new());
        for i in self.statements(m, true) {
            walk(&self.body[i], &mut strings, &mut requires);
        }
        (strings, requires)
    }

    /// The code excerpt: the wrapper body first (capped), then every other
    /// statement's first lines, deepened round-robin while they fit; cut
    /// at the budget when even two lines each do not.
    fn excerpt(&self, m: usize) -> String {
        let module = &self.modules[m];
        let init = &self.body[module.init_index];
        let head_lines = self.render(m, init, HEAD_LINES);
        let head = cut(&head_lines.join("\n"), HEAD_CHARS);
        let rest: Vec<Vec<String>> = self
            .statements(m, false)
            .into_iter()
            .map(|i| self.render(m, &self.body[i], STATEMENT_LINES))
            .filter(|lines| !lines.is_empty())
            .collect();
        let room = EXCERPT_CHARS.saturating_sub(head.chars().count());
        let shown = |k: usize| -> String {
            let mut out: Vec<String> = Vec::new();
            for s in &rest {
                out.extend(s.iter().take(k).cloned());
                if s.len() > k {
                    out.push("  …".into());
                }
            }
            out.join("\n")
        };
        let longest = rest.iter().map(Vec::len).max().unwrap_or(0);
        let mut k = 2;
        while k < longest && shown(k + 1).chars().count() <= room {
            k += 1;
        }
        let mut body = shown(k);
        if body.chars().count() > room {
            body = format!("{}\n…", cut(&body, room).trim_end_matches('…'));
        }
        let joined = if body.is_empty() {
            head
        } else {
            format!("{head}\n{body}")
        };
        joined.trim_matches('\n').to_string()
    }

    /// One module's evidence lines (everything after its `### key`).
    pub fn entry(&self, m: usize) -> String {
        let module = &self.modules[m];
        let mut lines: Vec<String> = Vec::new();
        let declared: Vec<String> = self
            .statements(m, false)
            .into_iter()
            .flat_map(|i| self.declaration_lines(m, &self.body[i]))
            .collect();
        lines.push(if declared.is_empty() {
            "Declares: nothing besides its setup code".to_string()
        } else {
            capped_list("Declares: ", &declared, MAX_DECLARED, "; ")
        });
        let assigns = self.assigns(m);
        if !assigns.is_empty() {
            lines.push(capped_list(
                "Its setup code assigns: ",
                &assigns,
                MAX_ASSIGNS,
                ", ",
            ));
        }
        if let Some(ns) = self.namespace(m) {
            lines.push(format!(
                "Other code reads this whole module as one object named: {ns}"
            ));
        }
        let mut seen = HashSet::new();
        let imports: Vec<String> = module
            .imports
            .iter()
            .filter(|&&j| j != m && seen.insert(j))
            .map(|&j| {
                let names: Vec<String> = self
                    .own_declared(j)
                    .into_iter()
                    .take(NAMES_PER_IMPORT)
                    .collect();
                names.join(", ")
            })
            .filter(|s| !s.is_empty())
            .collect();
        if !imports.is_empty() {
            lines.push(capped_list(
                "Imports modules that declare: ",
                &imports,
                MAX_IMPORTS,
                " | ",
            ));
        }
        let (strings, requires) = self.literals(m);
        let libraries: Vec<String> = requires.iter().filter_map(|r| library_label(r)).collect();
        if !libraries.is_empty() {
            let shown: Vec<String> = libraries.into_iter().take(MAX_LIBRARIES).collect();
            lines.push(format!("Uses libraries: {}", shown.join(", ")));
        }
        if !strings.is_empty() {
            let shown: Vec<String> = strings
                .iter()
                .take(MAX_STRINGS)
                .filter_map(|s| serde_json::to_string(s).ok())
                .collect();
            lines.push(format!("Strings: {}", shown.join(", ")));
        }
        lines.push(format!("Code:\n```js\n{}\n```", self.excerpt(m)));
        lines.join("\n")
    }
}

/// `head` + the first `max` items joined, with `… (N more)` past it.
fn capped_list(head: &str, items: &[String], max: usize, sep: &str) -> String {
    let shown: Vec<&str> = items.iter().take(max).map(String::as_str).collect();
    let mut line = format!("{head}{}", shown.join(sep));
    if items.len() > max {
        line.push_str(&format!("{sep}… ({} more)", items.len() - max));
    }
    line
}

/// `require("<spec>")` — a call of the free name `require` with one
/// string literal.
fn require_spec(v: &Value) -> Option<String> {
    if node_type(v) != "CallExpression" {
        return None;
    }
    let callee = v.get("callee")?;
    if node_type(callee) != "Identifier" || callee.get("name")?.as_str()? != "require" {
        return None;
    }
    match v.get("arguments")?.as_array()?.as_slice() {
        [arg] if node_type(arg) == "Literal" => arg.get("value")?.as_str().map(str::to_string),
        _ => None,
    }
}

/// What a required specifier says about the library: a bare package or
/// built-in as written (`node:` dropped); a vendored file by its path
/// below `vendor/`, content-hash folders dropped; an app path nothing.
fn library_label(spec: &str) -> Option<String> {
    if let Some(cut_at) = spec.rfind("vendor/") {
        let rest = spec[cut_at + "vendor/".len()..].trim_end_matches(".js");
        let named: Vec<&str> = rest.split('/').filter(|p| !is_hash_folder(p)).collect();
        return (!named.is_empty()).then(|| named.join("/"));
    }
    let bare = spec.strip_prefix("node:").unwrap_or(spec);
    (!bare.is_empty() && !bare.starts_with('.') && !bare.starts_with('/')).then(|| bare.to_string())
}

/// `lib_<8 hex>…` — the unpack's content-addressed vendor folders.
fn is_hash_folder(part: &str) -> bool {
    part.strip_prefix("lib_")
        .is_some_and(|h| h.len() >= 8 && h.chars().take(8).all(|c| c.is_ascii_hexdigit()))
}
