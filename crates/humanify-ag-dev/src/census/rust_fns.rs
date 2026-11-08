//! The censuses' ONE view of Rust source (`census clones` and
//! `census slop` both read it; there is no second parser or detector).
//!
//! **Which functions.** Every function with a body that is not inside
//! another function: free fns, inherent and trait-impl methods, trait
//! default methods, fns in inline `mod` blocks. Closures and fns nested
//! in a body belong to the function that holds them (the clone census's
//! TS predecessor took "module-level-ish" functions the same way).
//!
//! **Test or not.** A unit is TEST code when its file is test code
//! (`*_test.rs`, or under a crate's `tests/`), when it sits in a module
//! carrying `#[cfg(test)]`, or when it carries `#[test]` (any attribute
//! whose last path segment is `test`, e.g. `#[tokio::test]`) or
//! `#[cfg(test)]` itself. Everything else is NON-TEST. Both are scanned;
//! each census says how it treats them.
//!
//! **The masked stream.** A unit's signature and body (attributes and doc
//! comments excluded) as `proc_macro2` tokens, one per space-separated
//! word, with every BINDING name replaced by an order-keyed slot (`$0`,
//! `$1`, ... in order of first appearance): the fn's own name, its generic
//! type parameters, and every identifier bound by a pattern (parameters,
//! `let`, closure parameters, `for`, `match`/`if let` arms) that does not
//! start with an uppercase letter (an uppercase pattern identifier is a
//! unit variant or a constant, not a binding). An identifier right after
//! `.` or `::` (a field, a method, a path segment) stays verbatim even when
//! its spelling is also bound, and so do types, macros, free functions and
//! literals: twins with renamed locals group; twins calling a different
//! method do not. The BLURRED stream additionally replaces each literal by
//! its class and size (a string by its length, a number by its digit
//! count, a char by `'c'`) — the `--loose` review's near-twin key.
//!
//! **CC, rule `slop/1`** ([`CC_RULE`]): 1, plus one for each `if` (an
//! `else if` is an `if`; `if let` is an `if`), `while` (and `while let`),
//! `for`, `loop`, `&&` and `||`, plus one for each `match` arm after the
//! first. `?` is NOT counted; neither are `let ... else`, match guards, or
//! anything inside a macro invocation's arguments (syn leaves those as
//! unparsed tokens). Closures and nested fns inside the body count toward
//! the enclosing unit.
//!
//! **SLOC**: the lines from the `fn` keyword to the body's closing brace
//! that are neither blank nor a `//` comment line.

use std::collections::HashMap;
use std::path::Path;

use proc_macro2::{Delimiter, Spacing, TokenStream, TokenTree};
use quote::ToTokens;
use syn::visit::{self, Visit};

/// The version of the CC rule above; printed with every slop report.
pub const CC_RULE: &str = "slop/1";

/// Test code or not (see the module doc).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    NonTest,
    Test,
}

/// One function the censuses see.
#[derive(Clone, Debug)]
pub struct FnUnit {
    /// Path relative to the scanned root, e.g. `crates/k/src/a.rs`.
    pub rel: String,
    /// The crate directory name (`crates/<krate>/...`).
    pub krate: String,
    /// `Type::method`, `Trait::method`, `module::f` or `f`.
    pub name: String,
    pub line: usize,
    pub kind: Kind,
    pub sloc: usize,
    pub cc: u32,
    /// The masked stream, literal-exact (the strict grouping key).
    pub exact: String,
    /// The masked stream with literals blurred (the loose key).
    pub blurred: String,
    /// Tokens in the stream (the size the cutoffs measure).
    pub tokens: usize,
    /// The first lines of the function, for a report.
    pub excerpt: String,
}

impl FnUnit {
    /// The allowlist identity: `path:name`.
    pub fn id(&self) -> String {
        format!("{}:{}", self.rel, self.name)
    }
}

/// An `#[allow(clippy::cognitive_complexity)]` (or `expect`) site.
#[derive(Clone, Debug)]
pub struct AllowSite {
    pub rel: String,
    /// The item carrying it (`(file)` for an inner attribute of the file).
    pub name: String,
    pub line: usize,
}

/// Everything one file contributes.
#[derive(Clone, Debug, Default)]
pub struct FileFns {
    pub units: Vec<FnUnit>,
    pub allows: Vec<AllowSite>,
}

/// One source file: where it is and its text.
#[derive(Clone, Debug)]
pub struct Source {
    pub rel: String,
    pub text: String,
}

/// The crate a repo-relative path belongs to.
pub fn crate_of(rel: &str) -> String {
    let mut parts = rel.split('/');
    match (parts.next(), parts.next()) {
        (Some("crates"), Some(k)) => k.to_string(),
        _ => "(root)".to_string(),
    }
}

/// Whether a whole file is test code.
pub fn is_test_path(rel: &str) -> bool {
    let in_tests_dir = rel.split('/').skip(2).any(|c| c == "tests");
    in_tests_dir || rel.ends_with("_test.rs")
}

/// Directories a walk never enters.
fn skipped_dir(name: &str) -> bool {
    name == "target" || name == "node_modules" || name.starts_with('.')
}

fn walk(dir: &Path, root: &Path, out: &mut Vec<Source>) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("{}: {e}", dir.display()))?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if path.is_dir() {
            if !skipped_dir(&name) {
                walk(&path, root, out)?;
            }
        } else if name.ends_with(".rs") {
            let text =
                std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            let rel = path.strip_prefix(root).unwrap_or(&path);
            out.push(Source {
                rel: rel.to_string_lossy().replace('\\', "/"),
                text,
            });
        }
    }
    Ok(())
}

/// Every `.rs` file under `<root>/crates` in the working tree, sorted.
pub fn worktree_sources(root: &Path) -> Result<Vec<Source>, String> {
    let crates = root.join("crates");
    let mut out = Vec::new();
    if crates.is_dir() {
        walk(&crates, root, &mut out)?;
    }
    out.sort_by(|a, b| a.rel.cmp(&b.rel));
    Ok(out)
}

/// Every `.rs` file under `crates/` at git revision `rev` of the repo at
/// `root`, read from blobs (`git ls-tree` + one `git cat-file --batch`):
/// no worktree, no checkout. Sorted like [`worktree_sources`].
pub fn git_sources(root: &Path, rev: &str) -> Result<Vec<Source>, String> {
    let listing = crate::proc::git(root, &["ls-tree", "-r", "--name-only", rev, "--", "crates"])?;
    let rels: Vec<&str> = listing
        .lines()
        .filter(|p| p.ends_with(".rs"))
        .filter(|p| !p.split('/').any(skipped_dir))
        .collect();
    if rels.is_empty() {
        return Ok(Vec::new());
    }
    let stdin: String = rels.iter().map(|p| format!("{rev}:{p}\n")).collect();
    let mut spawn = crate::proc::Spawn::new(
        "git",
        &["-C", &root.display().to_string(), "cat-file", "--batch"],
        crate::proc::GIT_DEADLINE,
    );
    spawn.stdin = Some(stdin.into_bytes());
    let done = crate::proc::run(&spawn)?;
    if !done.status.success() {
        return Err(format!(
            "git cat-file --batch: {}",
            String::from_utf8_lossy(&done.stderr)
        ));
    }
    let texts = split_batch(&done.stdout)?;
    let mut out: Vec<Source> = rels
        .iter()
        .zip(texts)
        .map(|(rel, text)| Source {
            rel: (*rel).to_string(),
            text,
        })
        .collect();
    out.sort_by(|a, b| a.rel.cmp(&b.rel));
    Ok(out)
}

/// `git cat-file --batch` output: per object `<sha> blob <size>\n<bytes>\n`.
fn split_batch(mut bytes: &[u8]) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    while !bytes.is_empty() {
        let nl = bytes
            .iter()
            .position(|b| *b == b'\n')
            .ok_or("git cat-file: truncated header")?;
        let header = String::from_utf8_lossy(&bytes[..nl]).to_string();
        let size: usize = header
            .rsplit(' ')
            .next()
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| format!("git cat-file: {header}"))?;
        let body = bytes
            .get(nl + 1..nl + 1 + size)
            .ok_or("git cat-file: truncated body")?;
        out.push(String::from_utf8_lossy(body).to_string());
        bytes = bytes.get(nl + 2 + size..).unwrap_or(&[]);
    }
    Ok(out)
}

/// A whole tree's units and allow sites.
#[derive(Clone, Debug, Default)]
pub struct Scan {
    pub files: usize,
    pub units: Vec<FnUnit>,
    pub allows: Vec<AllowSite>,
}

/// Parse `sources` into one [`Scan`].
pub fn scan(sources: &[Source]) -> Result<Scan, String> {
    let mut out = Scan {
        files: sources.len(),
        ..Scan::default()
    };
    for f in parse_all(sources)? {
        out.units.extend(f.units);
        out.allows.extend(f.allows);
    }
    Ok(out)
}

/// Parse every source (in parallel through `par`, results in input order).
pub fn parse_all(sources: &[Source]) -> Result<Vec<FileFns>, String> {
    humanify_ag_core::par::map_ordered(sources, |s| parse_source(&s.rel, &s.text))
        .into_iter()
        .collect()
}

/// Parse one file into its units and allow sites.
pub fn parse_source(rel: &str, src: &str) -> Result<FileFns, String> {
    let file = syn::parse_file(src).map_err(|e| {
        let at = e.span().start();
        format!("{rel}:{}:{}: {e}", at.line, at.column + 1)
    })?;
    let lines: Vec<&str> = src.lines().collect();
    let mut c = Collector {
        rel,
        krate: crate_of(rel),
        lines: &lines,
        test: is_test_path(rel),
        prefix: Vec::new(),
        out: FileFns::default(),
    };
    if file.attrs.iter().any(is_cc_allow) {
        let line = file
            .attrs
            .iter()
            .find(|a| is_cc_allow(a))
            .map_or(1, attr_line);
        c.allow("(file)".to_string(), line);
    }
    c.visit_file(&file);
    Ok(c.out)
}

fn attr_line(a: &syn::Attribute) -> usize {
    a.pound_token.span.start().line
}

/// Every identifier anywhere in a token stream.
fn idents(ts: TokenStream) -> Vec<String> {
    let mut out = Vec::new();
    for tt in ts {
        match tt {
            TokenTree::Ident(i) => out.push(i.to_string()),
            TokenTree::Group(g) => out.extend(idents(g.stream())),
            _ => {}
        }
    }
    out
}

/// `#[test]`, `#[tokio::test]`, `#[cfg(test)]`, `#[cfg(any(test, ...))]`.
fn is_test_attr(a: &syn::Attribute) -> bool {
    let last = a.path().segments.last().map(|s| s.ident.to_string());
    if last.as_deref() == Some("test") {
        return true;
    }
    if !a.path().is_ident("cfg") {
        return false;
    }
    let words = idents(a.meta.to_token_stream());
    words.iter().any(|w| w == "test") && !words.iter().any(|w| w == "not")
}

fn is_cc_allow(a: &syn::Attribute) -> bool {
    let lint = a.path().is_ident("allow") || a.path().is_ident("expect");
    lint && idents(a.meta.to_token_stream())
        .iter()
        .any(|w| w == "cognitive_complexity")
}

fn type_name(ty: &syn::Type) -> String {
    match ty {
        syn::Type::Path(p) => p
            .path
            .segments
            .last()
            .map_or("(type)".to_string(), |s| s.ident.to_string()),
        syn::Type::Reference(r) => type_name(&r.elem),
        _ => "(type)".to_string(),
    }
}

struct Collector<'a> {
    rel: &'a str,
    krate: String,
    lines: &'a [&'a str],
    /// Inside test context (a test file or a `#[cfg(test)]` module).
    test: bool,
    /// Enclosing module / impl / trait names.
    prefix: Vec<String>,
    out: FileFns,
}

impl Collector<'_> {
    fn qualified(&self, ident: &syn::Ident) -> String {
        let mut parts = self.prefix.clone();
        parts.push(ident.to_string());
        parts.join("::")
    }

    fn allow(&mut self, name: String, line: usize) {
        self.out.allows.push(AllowSite {
            rel: self.rel.to_string(),
            name,
            line,
        });
    }

    fn note_allows(&mut self, attrs: &[syn::Attribute], name: &str) {
        if let Some(a) = attrs.iter().find(|a| is_cc_allow(a)) {
            self.allow(name.to_string(), attr_line(a));
        }
    }

    fn sloc(&self, start: usize, end: usize) -> usize {
        let hi = end.min(self.lines.len());
        let lo = start.max(1);
        if lo > hi {
            return 0;
        }
        self.lines[lo - 1..hi]
            .iter()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty() && !l.starts_with("//"))
            .count()
    }

    fn excerpt(&self, start: usize) -> String {
        let from = start.saturating_sub(1).min(self.lines.len());
        let text = self.lines[from..]
            .iter()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .take(3)
            .collect::<Vec<_>>()
            .join(" \\ ");
        text.chars().take(160).collect()
    }

    fn unit(&mut self, attrs: &[syn::Attribute], sig: &syn::Signature, block: &syn::Block) {
        let name = self.qualified(&sig.ident);
        self.note_allows(attrs, &name);
        let test = self.test || attrs.iter().any(is_test_attr);
        let start = sig.fn_token.span.start().line;
        let end = block.brace_token.span.close().end().line;
        let mut ts = sig.to_token_stream();
        ts.extend(block.to_token_stream());
        let bound = bindings(sig, block);
        let exact = Masker::new(&bound, false).run(ts.clone());
        let blurred = Masker::new(&bound, true).run(ts);
        let mut cc = Cc(1);
        cc.visit_block(block);
        self.out.units.push(FnUnit {
            rel: self.rel.to_string(),
            krate: self.krate.clone(),
            name,
            line: start,
            kind: if test { Kind::Test } else { Kind::NonTest },
            sloc: self.sloc(start, end),
            cc: cc.0,
            tokens: exact.len(),
            exact: exact.join(" "),
            blurred: blurred.join(" "),
            excerpt: self.excerpt(start),
        });
    }

    fn scoped(&mut self, name: String, test: bool, f: impl FnOnce(&mut Self)) {
        let was = self.test;
        self.test = was || test;
        self.prefix.push(name);
        f(self);
        self.prefix.pop();
        self.test = was;
    }
}

impl<'ast> Visit<'ast> for Collector<'_> {
    fn visit_item_fn(&mut self, i: &'ast syn::ItemFn) {
        self.unit(&i.attrs, &i.sig, &i.block);
    }

    fn visit_impl_item_fn(&mut self, i: &'ast syn::ImplItemFn) {
        self.unit(&i.attrs, &i.sig, &i.block);
    }

    fn visit_trait_item_fn(&mut self, i: &'ast syn::TraitItemFn) {
        if let Some(block) = &i.default {
            self.unit(&i.attrs, &i.sig, block);
        }
    }

    fn visit_item_mod(&mut self, i: &'ast syn::ItemMod) {
        let name = i.ident.to_string();
        self.note_allows(&i.attrs, &self.qualified(&i.ident));
        let test = i.attrs.iter().any(is_test_attr);
        self.scoped(name, test, |c| visit::visit_item_mod(c, i));
    }

    fn visit_item_impl(&mut self, i: &'ast syn::ItemImpl) {
        let name = type_name(&i.self_ty);
        self.note_allows(&i.attrs, &name);
        let test = i.attrs.iter().any(is_test_attr);
        self.scoped(name, test, |c| visit::visit_item_impl(c, i));
    }

    fn visit_item_trait(&mut self, i: &'ast syn::ItemTrait) {
        let name = i.ident.to_string();
        self.note_allows(&i.attrs, &name);
        let test = i.attrs.iter().any(is_test_attr);
        self.scoped(name, test, |c| visit::visit_item_trait(c, i));
    }
}

/// The binding names of one unit (see the module doc).
fn bindings(sig: &syn::Signature, block: &syn::Block) -> Vec<String> {
    let mut b = Bindings(vec![sig.ident.to_string()]);
    for p in sig.generics.type_params() {
        b.0.push(p.ident.to_string());
    }
    b.visit_signature(sig);
    b.visit_block(block);
    b.0
}

struct Bindings(Vec<String>);

impl<'ast> Visit<'ast> for Bindings {
    fn visit_pat_ident(&mut self, i: &'ast syn::PatIdent) {
        let name = i.ident.to_string();
        if !name.starts_with(|c: char| c.is_uppercase()) && name != "self" {
            self.0.push(name);
        }
        visit::visit_pat_ident(self, i);
    }
}

/// The token serializer: binding names to slots, literals exact or blurred.
struct Masker<'a> {
    bound: &'a [String],
    blur: bool,
    slots: HashMap<String, usize>,
    out: Vec<String>,
    /// The last two punctuation characters emitted (for `.x` and `::x`).
    prev: [Option<char>; 2],
}

impl<'a> Masker<'a> {
    fn new(bound: &'a [String], blur: bool) -> Masker<'a> {
        Masker {
            bound,
            blur,
            slots: HashMap::new(),
            out: Vec::new(),
            prev: [None, None],
        }
    }

    fn run(mut self, ts: TokenStream) -> Vec<String> {
        self.stream(ts);
        self.out
    }

    fn after_accessor(&self) -> bool {
        matches!(self.prev, [_, Some('.')] | [Some(':'), Some(':')])
    }

    fn ident(&mut self, name: String) {
        let masked = !self.after_accessor() && self.bound.contains(&name);
        let word = if masked {
            let next = self.slots.len();
            let slot = *self.slots.entry(name).or_insert(next);
            format!("${slot}")
        } else {
            name
        };
        self.push(word, None);
    }

    fn push(&mut self, word: String, punct: Option<char>) {
        self.prev = [self.prev[1], punct];
        self.out.push(word);
    }

    fn stream(&mut self, ts: TokenStream) {
        for tt in ts {
            match tt {
                TokenTree::Ident(i) => self.ident(i.to_string()),
                TokenTree::Punct(p) => {
                    let ch = p.as_char();
                    // A lone `:` (not half of `::`) resets the accessor memory.
                    let keep =
                        ch == ':' && (p.spacing() == Spacing::Joint || self.prev[1] == Some(':'));
                    let mark = if ch == '.' || keep { Some(ch) } else { None };
                    self.push(ch.to_string(), mark);
                }
                TokenTree::Literal(l) => {
                    let text = l.to_string();
                    let word = if self.blur { blur_literal(&text) } else { text };
                    self.push(word, None);
                }
                TokenTree::Group(g) => self.group(g.delimiter(), g.stream()),
            }
        }
    }

    fn group(&mut self, delim: Delimiter, inner: TokenStream) {
        let (open, close) = match delim {
            Delimiter::Parenthesis => ("(", ")"),
            Delimiter::Brace => ("{", "}"),
            Delimiter::Bracket => ("[", "]"),
            Delimiter::None => ("", ""),
        };
        if !open.is_empty() {
            self.push(open.to_string(), None);
        }
        self.stream(inner);
        if !close.is_empty() {
            self.push(close.to_string(), None);
        }
    }
}

/// A literal's class and size: `"<len>"`, `'c'`, `N<digits>`.
fn blur_literal(text: &str) -> String {
    let first = text.chars().next().unwrap_or(' ');
    if first == '\'' || text.starts_with("b'") {
        return "'c'".to_string();
    }
    if first.is_ascii_digit() || first == '-' {
        let digits = text
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '.')
            .count();
        return format!("N{digits}");
    }
    format!("\"{}\"", text.len())
}

/// The CC visitor, rule `slop/1`.
struct Cc(u32);

impl<'ast> Visit<'ast> for Cc {
    fn visit_expr_if(&mut self, i: &'ast syn::ExprIf) {
        self.0 += 1;
        visit::visit_expr_if(self, i);
    }
    fn visit_expr_while(&mut self, i: &'ast syn::ExprWhile) {
        self.0 += 1;
        visit::visit_expr_while(self, i);
    }
    fn visit_expr_for_loop(&mut self, i: &'ast syn::ExprForLoop) {
        self.0 += 1;
        visit::visit_expr_for_loop(self, i);
    }
    fn visit_expr_loop(&mut self, i: &'ast syn::ExprLoop) {
        self.0 += 1;
        visit::visit_expr_loop(self, i);
    }
    fn visit_expr_binary(&mut self, i: &'ast syn::ExprBinary) {
        if matches!(i.op, syn::BinOp::And(_) | syn::BinOp::Or(_)) {
            self.0 += 1;
        }
        visit::visit_expr_binary(self, i);
    }
    fn visit_expr_match(&mut self, i: &'ast syn::ExprMatch) {
        self.0 += u32::try_from(i.arms.len().saturating_sub(1)).unwrap_or(u32::MAX);
        visit::visit_expr_match(self, i);
    }
}

#[cfg(test)]
#[path = "rust_fns_test.rs"]
pub(crate) mod rust_fns_test;
