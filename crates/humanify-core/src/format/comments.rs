//! Babel's comment ATTACHMENT (@babel/parser 7.29.7 `CommentsParser`:
//! `processComment` at every `finishNode`, `finalizeComment`,
//! `takeSurroundingComments` at a parenthesized expression,
//! `finalizeRemainingComments`), then @babel/core's `normalizeFile`
//! dropping the `//# sourceMappingURL=` comments.
//!
//! With `comments: false` nothing here is printed (only an `@license` /
//! `@preserve` comment would be, and `super::format_file` prints those as
//! a file header — [`license_comments`], finding #46), but
//! attached comments still decide output bytes: parentheses kept around a
//! parenthesized expression with a leading block comment, `(`…`)` at a
//! no-line-terminator position with a newline comment, an arrow's lone
//! parameter keeping its parentheses, an if-branch printed one indent
//! deeper (see `super::printer`).
//!
//! The parser attaches as it finishes nodes; this replays that on the
//! converted tree: a post-order walk (children in source order), each node
//! finished at its Babel `end`, with every comment whitespace run (a
//! maximal run of whitespace and comments between two tokens) pushed on
//! the stack once the parser's lookahead has lexed it (its start ≤ the
//! finishing node's end). Babel's throwaway nodes (an `async` / `get` /
//! `set` / `static` keyword first parsed as an identifier) have no
//! counterpart here, which is what the parser's own
//! `resetPreviousNodeTrailingComments` calls restore.

use oxc_ast::ast::Program;

use super::ast::{Comment, Kind, Loc, NodeId, Tree};
use crate::babel_view::BabelLines;

/// A `CommentWhitespace`.
struct Ws {
    start: u32,
    end: u32,
    comments: Vec<u32>,
    leading_node: Option<NodeId>,
    trailing_node: Option<NodeId>,
    containing_node: Option<NodeId>,
}

fn is_js_space(c: char) -> bool {
    humanify_model::js::is_js_whitespace(c)
}

/// The comment runs of `code`, in source order.
fn whitespace_runs(code: &str, comments: &[Comment]) -> Vec<Ws> {
    let mut runs: Vec<Ws> = Vec::new();
    for (i, c) in comments.iter().enumerate() {
        let joined = runs.last().is_some_and(|r| {
            r.end >= c.start
                || code[r.end as usize..c.start as usize]
                    .chars()
                    .all(is_js_space)
        });
        let mut end = c.end as usize;
        end += code[end..]
            .chars()
            .take_while(|&ch| is_js_space(ch))
            .map(char::len_utf8)
            .sum::<usize>();
        if joined {
            let r = runs.last_mut().expect("joined");
            r.comments.push(i as u32);
            r.end = end as u32;
            continue;
        }
        let mut start = c.start as usize;
        start -= code[..start]
            .chars()
            .rev()
            .take_while(|&ch| is_js_space(ch))
            .map(char::len_utf8)
            .sum::<usize>();
        runs.push(Ws {
            start: start as u32,
            end: end as u32,
            comments: vec![i as u32],
            leading_node: None,
            trailing_node: None,
            containing_node: None,
        });
    }
    runs
}

struct Attacher<'t> {
    tree: &'t mut Tree,
    code: &'t str,
    runs: Vec<Ws>,
    next_run: usize,
    stack: Vec<Ws>,
}

impl Attacher<'_> {
    /// The parser's lookahead has lexed every run starting at or before
    /// `end`.
    fn lex_to(&mut self, end: u32) {
        while self.next_run < self.runs.len() && self.runs[self.next_run].start <= end {
            let run = std::mem::replace(
                &mut self.runs[self.next_run],
                Ws {
                    start: 0,
                    end: 0,
                    comments: Vec::new(),
                    leading_node: None,
                    trailing_node: None,
                    containing_node: None,
                },
            );
            self.stack.push(run);
            self.next_run += 1;
        }
    }

    /// `processComment(node)`.
    fn process(&mut self, node: NodeId, start: u32, end: u32) {
        self.lex_to(end);
        if self.stack.is_empty() {
            return;
        }
        let mut i = self.stack.len() as isize - 1;
        if self.stack[i as usize].start == end {
            self.stack[i as usize].leading_node = Some(node);
            i -= 1;
        }
        while i >= 0 {
            let idx = i as usize;
            if self.stack[idx].end > start {
                self.stack[idx].containing_node = Some(node);
                let ws = self.stack.remove(idx);
                self.finalize(ws);
            } else {
                if self.stack[idx].end == start {
                    self.stack[idx].trailing_node = Some(node);
                }
                break;
            }
            i -= 1;
        }
    }

    /// `takeSurroundingComments(node, start, end)` (a parenthesized
    /// expression's parentheses).
    fn take_surrounding(&mut self, node: NodeId, start: u32, end: u32) {
        self.lex_to(end);
        for ws in self.stack.iter_mut().rev() {
            if ws.start == end {
                ws.leading_node = Some(node);
            } else if ws.end == start {
                ws.trailing_node = Some(node);
            } else if ws.end < start {
                break;
            }
        }
    }

    fn comments_mut(&mut self, n: NodeId) -> &mut super::ast::NodeComments {
        self.tree
            .node_mut(n)
            .comments
            .get_or_insert_with(Default::default)
    }

    /// `finalizeComment(commentWS)`.
    fn finalize(&mut self, ws: Ws) {
        if ws.leading_node.is_some() || ws.trailing_node.is_some() {
            if let Some(n) = ws.leading_node {
                // setTrailingComments: assign, or unshift onto existing.
                let t = &mut self.comments_mut(n).trailing;
                let mut merged = ws.comments.clone();
                merged.extend(t.iter().copied());
                *t = merged;
            }
            if let Some(n) = ws.trailing_node {
                let l = &mut self.comments_mut(n).leading;
                let mut merged = ws.comments.clone();
                merged.extend(l.iter().copied());
                *l = merged;
            }
            return;
        }
        let Some(node) = ws.containing_node else {
            return;
        };
        let after_comma = ws.start > 0 && self.code.as_bytes()[ws.start as usize - 1] == b',';
        let elements: Option<Vec<NodeId>> = if after_comma {
            match self.tree.kind(node) {
                Kind::ObjectExpression { properties } | Kind::ObjectPattern { properties } => {
                    Some(properties.clone())
                }
                Kind::CallExpression(c)
                | Kind::NewExpression(c)
                | Kind::OptionalCallExpression(c) => Some(c.arguments.clone()),
                Kind::FunctionDeclaration(f)
                | Kind::FunctionExpression(f)
                | Kind::ArrowFunctionExpression(f) => Some(f.params.clone()),
                Kind::ObjectMethod(m) | Kind::ClassMethod(m) | Kind::ClassPrivateMethod(m) => {
                    Some(m.func.params.clone())
                }
                Kind::ArrayExpression { elements } | Kind::ArrayPattern { elements } => {
                    Some(elements.clone())
                }
                Kind::ExportNamedDeclaration { specifiers, .. }
                | Kind::ImportDeclaration { specifiers, .. } => Some(specifiers.clone()),
                _ => None,
            }
        } else {
            None
        };
        // `adjustInnerComments`: after a trailing comma, the comments
        // trail the last element when it starts before them.
        if let Some(elements) = elements {
            let last = elements.iter().rev().find(|e| !e.is_none()).copied();
            if let Some(last) = last
                && self
                    .tree
                    .node(last)
                    .span
                    .is_some_and(|(s, _)| s <= ws.start)
            {
                let t = &mut self.comments_mut(last).trailing;
                let mut merged = ws.comments;
                merged.extend(t.iter().copied());
                *t = merged;
                return;
            }
        }
        let inner = &mut self.comments_mut(node).inner;
        let mut merged = ws.comments;
        merged.extend(inner.iter().copied());
        *inner = merged;
    }

    /// The node's Babel [start, end]; None for a node the parser never
    /// finished (synthesized).
    fn babel_span(&self, node: NodeId) -> Option<(u32, u32)> {
        self.tree.node(node).span
    }

    /// Post-order: children in source order, then the node itself, then
    /// its parentheses (innermost first).
    fn walk(&mut self, node: NodeId, parens: &[Vec<(u32, u32)>]) {
        let mut children = self.tree.children(node);
        children.sort_by_key(|&c| self.tree.node(c).span.map_or(0, |(s, _)| s));
        for c in children {
            self.walk(c, parens);
        }
        if let Some((start, end)) = self.babel_span(node) {
            self.process(node, start, end);
            if let Some(ps) = parens.get(node.0 as usize) {
                for &(s, e) in ps {
                    self.take_surrounding(node, s, e);
                }
            }
        }
    }
}

/// `//# sourceMappingURL=data:…;base64,…` (normalizeFile's inline regex,
/// over the comment's value).
fn is_inline_source_map(value: &str) -> bool {
    let Some(rest) = value.strip_prefix(['@', '#']) else {
        return false;
    };
    let trimmed = rest.trim_start_matches(|c: char| c.is_whitespace());
    if trimmed.len() == rest.len() {
        return false;
    }
    let Some(url) = trimmed.strip_prefix("sourceMappingURL=data:") else {
        return false;
    };
    let Some(url) = url
        .strip_prefix("application/json;")
        .or_else(|| url.strip_prefix("text/json;"))
    else {
        return false;
    };
    let url = match url.strip_prefix("charset") {
        Some(r) if r.starts_with([':', '=']) => match r.find(';') {
            Some(i) if i > 1 => &r[i + 1..],
            _ => return false,
        },
        _ => url,
    };
    url.starts_with("base64,") && !url.contains(['\n', '\r', '\u{2028}', '\u{2029}'])
}

/// `^[@#][ \t]+sourceMappingURL=([^\s'"`]+)[ \t]*$` (the external regex).
fn is_external_source_map(value: &str) -> bool {
    let Some(rest) = value.strip_prefix(['@', '#']) else {
        return false;
    };
    let trimmed = rest.trim_start_matches([' ', '\t']);
    if trimmed.len() == rest.len() {
        return false;
    }
    let Some(url) = trimmed.strip_prefix("sourceMappingURL=") else {
        return false;
    };
    let url = url.trim_end_matches([' ', '\t']);
    !url.is_empty()
        && !url
            .chars()
            .any(|c| c.is_whitespace() || c == '\'' || c == '"' || c == '`')
}

/// Attach the program's comments to the tree's nodes (root: the File).
pub fn attach(
    tree: &mut Tree,
    code: &str,
    program: &Program<'_>,
    root: NodeId,
) -> Result<(), String> {
    if program.comments.is_empty() {
        return Ok(());
    }
    let lines = BabelLines::new(code);
    for c in &program.comments {
        let content = c.content_span();
        let value = code[content.start as usize..content.end as usize].to_string();
        tree.comments.push(Comment {
            block: c.is_block(),
            value,
            start: c.span.start,
            end: c.span.end,
            loc: Loc {
                start: lines.line(c.span.start) as u32,
                end: lines.line(c.span.end) as u32,
            },
        });
    }
    let comments = tree.comments.clone();
    let runs = whitespace_runs(code, &comments);
    // Babel's Program / File end at the last token, not at EOF.
    let program_id = match tree.kind(root) {
        Kind::File { program } => *program,
        _ => root,
    };
    let last_end = tree
        .children(program_id)
        .iter()
        .filter_map(|&c| tree.node(c).span.map(|(_, e)| e))
        .max()
        .unwrap_or(0);
    tree.node_mut(program_id).span = Some((0, last_end));
    tree.node_mut(root).span = Some((0, last_end));
    let mut parens: Vec<Vec<(u32, u32)>> = Vec::new();
    for &(node, s, e) in &tree.parens {
        let i = node.0 as usize;
        if parens.len() <= i {
            parens.resize(i + 1, Vec::new());
        }
        parens[i].push((s, e));
    }
    let mut a = Attacher {
        tree,
        code,
        runs,
        next_run: 0,
        stack: Vec::new(),
    };
    a.walk(root, &parens);
    a.lex_to(u32::MAX);
    // finalizeRemainingComments
    while let Some(ws) = a.stack.pop() {
        a.finalize(ws);
    }
    // The File node was synthesized: its span was only for the replay.
    a.tree.node_mut(root).span = None;
    drop_source_map_comments(a.tree);
    Ok(())
}

/// normalizeFile: drop the inline source-map comments, then (none found)
/// the external ones, from every attachment list (an emptied list stays,
/// as Babel's filtered array does).
fn drop_source_map_comments(tree: &mut Tree) {
    let inline: Vec<u32> = (0..tree.comments.len() as u32)
        .filter(|&i| is_inline_source_map(&tree.comments[i as usize].value))
        .collect();
    let drop: Vec<u32> = if inline.is_empty() {
        (0..tree.comments.len() as u32)
            .filter(|&i| is_external_source_map(&tree.comments[i as usize].value))
            .collect()
    } else {
        inline
    };
    if drop.is_empty() {
        return;
    }
    for node in &mut tree.nodes {
        if let Some(c) = node.comments.as_mut() {
            c.leading.retain(|i| !drop.contains(i));
            c.trailing.retain(|i| !drop.contains(i));
            c.inner.retain(|i| !drop.contains(i));
        }
    }
}

/// The comments Babel's `shouldPrintComment` keeps with `comments: false`
/// — exactly the `@license` / `@preserve` ones — in source order, each as
/// its source text (`/*…*/` or `//…`). Finding #46: the formatter prints
/// them as a header (see `super::format_file`).
pub fn license_comments(tree: &Tree) -> Vec<String> {
    tree.comments
        .iter()
        .filter(|c| c.value.contains("@license") || c.value.contains("@preserve"))
        .map(comment_text)
        .collect()
}

fn comment_text(c: &Comment) -> String {
    if c.block {
        format!("/*{}*/", c.value)
    } else {
        format!("//{}", c.value)
    }
}
