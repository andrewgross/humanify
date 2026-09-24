//! The `env-reads` subcommand (TS: `src/commands/env-reads.ts`,
//! `src/env-reads/analyze.ts`, `src/env-reads/format.ts`): a static
//! inventory of `process.env` / `Bun.env` / `import.meta.env` reads in a
//! file or a tree.
//!
//! The walk is Babel's, translated onto oxc (lessons 2, 9, 14):
//! - a base is a PLAIN member expression `<process|Bun|import.meta>.env` —
//!   Babel's `MemberExpression` visitor never sees an optional link, so
//!   `process?.env` is not a base, and a base whose parent link is optional
//!   (`process.env?.X`) is an enumerated use, not a variable read;
//! - a `process`/`Bun` that resolves to a binding is a shadow, skipped;
//! - an alias (`const e = process.env`) is followed through Babel's
//!   `referencePaths` (the oxc references minus assignment targets — the
//!   one owner, `humanify_core::graph::babel_reference_node_ids`);
//! - locations are Babel's: 1-based lines split on `\r\n`, `\r`, `\n`,
//!   U+2028, U+2029; 0-based columns in UTF-16 units; snippets are the first
//!   80 UTF-16 units, whitespace runs collapsed with JS `\s`, JS-trimmed;
//! - the report sorts names and files with `localeCompare` (ICU order,
//!   `humanify_model::js::locale_compare`), not bytes.
//!
//! Gate: test/parity/wpb4-env-reads.sh runs both binaries over a corpus and
//! byte-compares the text and Markdown reports.

use std::path::Path;

use humanify_model::js::{cmp_utf16, is_js_whitespace, locale_compare};
use oxc_ast::AstKind;
use oxc_ast::ast::{BindingPattern, Expression, PropertyKey};
use oxc_semantic::{NodeId, Semantic, SemanticBuilder};
use oxc_span::{GetSpan, Span};

/// One read site.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnvLocation {
    pub file: String,
    pub line: u32,
    pub column: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnvVarReads {
    pub name: String,
    pub locations: Vec<EnvLocation>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnvSiteUse {
    pub loc: EnvLocation,
    pub snippet: String,
}

/// TS `EnvReadsReport`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EnvReadsReport {
    pub by_var: Vec<EnvVarReads>,
    pub dynamic: Vec<EnvSiteUse>,
    pub enumerated: Vec<EnvSiteUse>,
    pub files_analyzed: usize,
}

enum Finding {
    Var(String, EnvLocation),
    Dynamic(EnvSiteUse),
    Enumerated(EnvSiteUse),
}

/// Babel's position model over one source text.
struct Positions<'s> {
    code: &'s str,
    /// Byte offset of each line's first character.
    line_starts: Vec<usize>,
}

impl<'s> Positions<'s> {
    fn new(code: &'s str) -> Self {
        let mut line_starts = vec![0];
        let mut iter = code.char_indices().peekable();
        while let Some((i, c)) = iter.next() {
            match c {
                '\r' => {
                    if let Some((_, '\n')) = iter.peek() {
                        iter.next();
                        line_starts.push(i + 2);
                    } else {
                        line_starts.push(i + 1);
                    }
                }
                '\n' => line_starts.push(i + 1),
                '\u{2028}' | '\u{2029}' => line_starts.push(i + c.len_utf8()),
                _ => {}
            }
        }
        Positions { code, line_starts }
    }

    fn loc(&self, file: &str, offset: u32) -> EnvLocation {
        let offset = offset as usize;
        let line = self.line_starts.partition_point(|s| *s <= offset);
        let start = self.line_starts[line - 1];
        EnvLocation {
            file: file.to_string(),
            line: line as u32,
            column: self.code[start..offset].encode_utf16().count() as u32,
        }
    }

    /// `snippetOf`: `code.slice(start, min(end, start + 80))` in UTF-16
    /// units, `/\s+/g` → " ", trimmed.
    fn snippet(&self, span: Span) -> String {
        let text = &self.code[span.start as usize..span.end as usize];
        let mut out = String::new();
        let mut units = 0usize;
        for c in text.chars() {
            let n = c.len_utf16();
            if units + n > 80 {
                if units < 80 {
                    // A lone high surrogate, as stdout encodes it.
                    out.push('\u{FFFD}');
                }
                break;
            }
            out.push(c);
            units += n;
        }
        let mut collapsed = String::new();
        let mut in_ws = false;
        for c in out.chars() {
            if is_js_whitespace(c) {
                if !in_ws {
                    collapsed.push(' ');
                }
                in_ws = true;
            } else {
                collapsed.push(c);
                in_ws = false;
            }
        }
        humanify_model::js::trim(&collapsed).to_string()
    }
}

struct Ctx<'a, 's> {
    file: &'s str,
    pos: Positions<'s>,
    semantic: &'a Semantic<'a>,
    findings: Vec<Finding>,
    seen: std::collections::HashSet<NodeId>,
}

impl Ctx<'_, '_> {
    fn site(&self, span: Span) -> EnvSiteUse {
        EnvSiteUse {
            loc: self.pos.loc(self.file, span.start),
            snippet: self.pos.snippet(span),
        }
    }

    fn enumerated(&mut self, span: Span) {
        let s = self.site(span);
        self.findings.push(Finding::Enumerated(s));
    }
}

/// `isEnvBase` + not optional: `<process|Bun|import.meta>.env`.
fn env_base_object<'k, 'a>(
    kind: &'k AstKind<'a>,
) -> Option<Option<&'k oxc_ast::ast::IdentifierReference<'a>>> {
    let AstKind::StaticMemberExpression(m) = kind else {
        return None;
    };
    if m.optional || m.property.name != "env" {
        return None;
    }
    match &m.object {
        Expression::Identifier(id) if id.name == "process" || id.name == "Bun" => Some(Some(id)),
        Expression::ImportMeta(_) => Some(None),
        _ => None,
    }
}

fn is_shadowed(semantic: &Semantic<'_>, id: &oxc_ast::ast::IdentifierReference<'_>) -> bool {
    id.reference_id
        .get()
        .is_some_and(|r| semantic.scoping().get_reference(r).symbol_id().is_some())
}

/// `classifyUse`: one env-object occurrence (a base, or an alias
/// reference), by its parent.
fn classify_use(ctx: &mut Ctx<'_, '_>, node_id: NodeId) {
    if !ctx.seen.insert(node_id) {
        return;
    }
    let nodes = ctx.semantic.nodes();
    let span = nodes.get_node(node_id).kind().span();
    let parent_id = nodes.parent_id(node_id);
    match nodes.get_node(parent_id).kind() {
        // `parent.isMemberExpression() && parent.node.object === node`,
        // where Babel's MemberExpression is a NON-optional link.
        AstKind::StaticMemberExpression(m) if !m.optional && m.object.span() == span => {
            let loc = ctx.pos.loc(ctx.file, m.span.start);
            ctx.findings
                .push(Finding::Var(m.property.name.to_string(), loc));
        }
        AstKind::ComputedMemberExpression(m) if !m.optional && m.object.span() == span => {
            match &m.expression {
                Expression::StringLiteral(s) => {
                    let loc = ctx.pos.loc(ctx.file, m.span.start);
                    ctx.findings.push(Finding::Var(s.value.to_string(), loc));
                }
                _ => {
                    let s = ctx.site(m.span);
                    ctx.findings.push(Finding::Dynamic(s));
                }
            }
        }
        AstKind::VariableDeclarator(d) if d.init.as_ref().is_some_and(|i| i.span() == span) => {
            classify_declarator(ctx, d, span);
        }
        _ => ctx.enumerated(span),
    }
}

/// `classifyDeclarator`: destructure, alias, or (odd) whole.
fn classify_declarator(
    ctx: &mut Ctx<'_, '_>,
    decl: &oxc_ast::ast::VariableDeclarator<'_>,
    init_span: Span,
) {
    match &decl.id {
        BindingPattern::ObjectPattern(p) => {
            for prop in &p.properties {
                let key_name = match &prop.key {
                    PropertyKey::StaticIdentifier(id) if !prop.computed => {
                        Some(id.name.to_string())
                    }
                    PropertyKey::StringLiteral(s) => Some(s.value.to_string()),
                    _ => None,
                };
                match key_name {
                    Some(name) => {
                        let loc = ctx.pos.loc(ctx.file, prop.span.start);
                        ctx.findings.push(Finding::Var(name, loc));
                    }
                    None => {
                        let s = ctx.site(prop.span);
                        ctx.findings.push(Finding::Dynamic(s));
                    }
                }
            }
            if let Some(rest) = &p.rest {
                ctx.enumerated(rest.span);
            }
        }
        BindingPattern::BindingIdentifier(id) => {
            let Some(symbol) = id.symbol_id.get() else {
                return;
            };
            for node in humanify_core::graph::babel_reference_node_ids(ctx.semantic, symbol) {
                classify_use(ctx, node);
            }
        }
        _ => ctx.enumerated(init_span),
    }
}

/// `findEnvReads` over one file. A parse failure is an error (the TS
/// parser throws, and the command dies on it).
fn find_env_reads(code: &str, file: &str, out: &mut Vec<Finding>) -> Result<(), String> {
    let allocator = oxc_allocator::Allocator::default();
    let ret = oxc_parser::Parser::new(&allocator, code, oxc_span::SourceType::unambiguous())
        .with_options(oxc_parser::ParseOptions {
            preserve_parens: false,
            ..oxc_parser::ParseOptions::default()
        })
        .parse();
    if let Some(e) = ret.diagnostics.first() {
        return Err(format!("{file}: {e}"));
    }
    let program = allocator.alloc(ret.program);
    let semantic = SemanticBuilder::new()
        .with_build_nodes(true)
        .build(program)
        .semantic;
    let mut ctx = Ctx {
        file,
        pos: Positions::new(code),
        semantic: &semantic,
        findings: Vec::new(),
        seen: std::collections::HashSet::new(),
    };
    let bases: Vec<NodeId> = semantic
        .nodes()
        .iter()
        .filter(|n| match env_base_object(&n.kind()) {
            Some(Some(id)) => !is_shadowed(&semantic, id),
            Some(None) => true,
            None => false,
        })
        .map(|n| n.id())
        .collect();
    for id in bases {
        classify_use(&mut ctx, id);
    }
    out.append(&mut ctx.findings);
    Ok(())
}

fn by_location(a: &EnvLocation, b: &EnvLocation) -> std::cmp::Ordering {
    locale_compare(&a.file, &b.file)
        .then(a.line.cmp(&b.line))
        .then(a.column.cmp(&b.column))
}

/// `analyzeEnvReads(inputs)`.
pub fn analyze_env_reads(inputs: &[(String, String)]) -> Result<EnvReadsReport, String> {
    let mut findings = Vec::new();
    for (file, code) in inputs {
        find_env_reads(code, file, &mut findings)?;
    }
    let mut vars: Vec<(String, Vec<EnvLocation>)> = Vec::new();
    let mut report = EnvReadsReport {
        files_analyzed: inputs.len(),
        ..EnvReadsReport::default()
    };
    for f in findings {
        match f {
            Finding::Var(name, loc) => match vars.iter_mut().find(|(n, _)| *n == name) {
                Some((_, locs)) => locs.push(loc),
                None => vars.push((name, vec![loc])),
            },
            Finding::Dynamic(s) => report.dynamic.push(s),
            Finding::Enumerated(s) => report.enumerated.push(s),
        }
    }
    report.by_var = vars
        .into_iter()
        .map(|(name, mut locations)| {
            locations.sort_by(by_location);
            EnvVarReads { name, locations }
        })
        .collect();
    report
        .by_var
        .sort_by(|a, b| locale_compare(&a.name, &b.name));
    report.dynamic.sort_by(|a, b| by_location(&a.loc, &b.loc));
    report
        .enumerated
        .sort_by(|a, b| by_location(&a.loc, &b.loc));
    Ok(report)
}

fn fmt_loc(loc: &EnvLocation) -> String {
    format!("{}:{}", loc.file, loc.line)
}

fn text_sites(title: &str, note: &str, uses: &[EnvSiteUse]) -> Vec<String> {
    if uses.is_empty() {
        return Vec::new();
    }
    let mut lines = vec![format!("{title} ({}){note}:", uses.len())];
    for u in uses {
        lines.push(format!("  {}  {}", fmt_loc(&u.loc), u.snippet));
    }
    lines.push(String::new());
    lines
}

fn md_sites(title: &str, note: &str, uses: &[EnvSiteUse]) -> Vec<String> {
    if uses.is_empty() {
        return Vec::new();
    }
    let mut lines = vec![format!("## {title} ({})", uses.len()), String::new()];
    if !note.is_empty() {
        lines.push(note.to_string());
        lines.push(String::new());
    }
    for u in uses {
        lines.push(format!("- {} — `{}`", fmt_loc(&u.loc), u.snippet));
    }
    lines.push(String::new());
    lines
}

/// `formatEnvReadsReport(report, {markdown})`: `join("\n").trimEnd() + "\n"`.
pub fn format_env_reads_report(report: &EnvReadsReport, markdown: bool) -> String {
    let mut out: Vec<String> = Vec::new();
    let n = report.by_var.len();
    if markdown {
        out.extend([
            "# Environment variable reads".to_string(),
            String::new(),
            format!("{} file(s), {n} variable(s).", report.files_analyzed),
            String::new(),
        ]);
        if n > 0 {
            out.push(format!("## Variables ({n})"));
            out.push(String::new());
            for v in &report.by_var {
                let locs: Vec<String> = v.locations.iter().map(fmt_loc).collect();
                out.push(format!("- `{}` — {}", v.name, locs.join(", ")));
            }
            out.push(String::new());
        }
        out.extend(md_sites(
            "Dynamic keys",
            "Computed at runtime, not statically resolvable.",
            &report.dynamic,
        ));
        out.extend(md_sites(
            "Whole-env / enumerated uses",
            "",
            &report.enumerated,
        ));
    } else {
        out.push(format!(
            "Environment variable reads — {} file(s), {n} variable(s)",
            report.files_analyzed
        ));
        out.push(String::new());
        if n > 0 {
            out.push(format!("Variables ({n}):"));
            for v in &report.by_var {
                out.push(format!("  {}", v.name));
                for loc in &v.locations {
                    out.push(format!("    {}", fmt_loc(loc)));
                }
            }
            out.push(String::new());
        }
        out.extend(text_sites(
            "Dynamic keys",
            " — computed at runtime, not statically resolvable",
            &report.dynamic,
        ));
        out.extend(text_sites(
            "Whole-env / enumerated uses",
            "",
            &report.enumerated,
        ));
    }
    let joined = out.join("\n");
    format!("{}\n", joined.trim_end_matches(is_js_whitespace))
}

/// `collectInputs`: one file as given, or every .js/.cjs/.mjs under a
/// directory (relative labels, sorted like `Array.prototype.sort`).
fn collect_inputs(input: &str) -> Result<Vec<(String, String)>, String> {
    let read = |p: &Path| {
        std::fs::read(p)
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .map_err(|e| format!("{}: {e}", p.display()))
    };
    let path = Path::new(input);
    if path.is_dir() {
        let mut rels: Vec<String> =
            crate::util::list_js_files_recursive(path, path, &[".js", ".cjs", ".mjs"])
                .into_iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect();
        rels.sort_by(|a, b| cmp_utf16(a, b));
        rels.into_iter()
            .map(|rel| read(&path.join(&rel)).map(|code| (rel, code)))
            .collect()
    } else {
        Ok(vec![(input.to_string(), read(path)?)])
    }
}

/// The subcommand's action. Returns the exit code.
pub fn run(input: &str, markdown: bool, output: Option<&str>) -> i32 {
    if !Path::new(input).exists() {
        eprintln!("Error: path does not exist: {input}");
        return 1;
    }
    let result = collect_inputs(input)
        .and_then(|inputs| analyze_env_reads(&inputs).map(|r| (inputs.len(), r)));
    let (files, report) = match result {
        Ok(x) => x,
        Err(e) => {
            eprintln!("Error: {e}");
            return 1;
        }
    };
    let text = format_env_reads_report(&report, markdown);
    match output {
        Some(dest) => {
            if let Err(e) = std::fs::write(dest, &text) {
                eprintln!("Error: {dest}: {e}");
                return 1;
            }
            eprintln!(
                "Wrote env-reads report to {dest} ({} variable(s), {files} file(s))",
                report.by_var.len()
            );
        }
        None => {
            use std::io::Write;
            let _ = std::io::stdout().write_all(text.as_bytes());
        }
    }
    0
}
