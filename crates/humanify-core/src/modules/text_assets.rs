//! App TEXT assets (finding #90): a bundled module that is nothing but a
//! piece of the app's own text — a prompt, a bundled script, a document —
//! is app content, not a third-party library.
//!
//! A bundler wraps any `require()`d file as a module, so an app that
//! requires its own text files (`.md`, `.txt`, a script read as a string)
//! gets one CommonJS module per file, and the unpack used to vendor them
//! with every other factory. They then went to the PACKAGE namer, which
//! has nothing to go on but prose: Claude Code's 29 text modules at
//! 2.1.216 were named `react`, `storybook`, `postcss`, `esbuild`,
//! `eslint-plugin-security`, … and one was named `pypa` by the URL rule
//! (its text links pypa.io).
//!
//! The rule is two facts the bundle states, nothing app-specific:
//!
//! - **shape** ([`exported_text`]): the module's body is exactly
//!   `module.exports = <text>` — a string literal, or a template literal
//!   with no `${}` — where `module` is the factory's own second parameter;
//! - **who requires it**: only app code. A module some vendored module
//!   requires is part of that library and stays vendored (decided by the
//!   unpack, which knows every reference — `unpack::bun`).
//!
//! On the eval's versions the split is clean at every version: 3 / 5 / 29 /
//! 29 text modules at 2.1.86 / 119 / 198 / 216, every one required only by
//! app code (/work/vendor-mapping-2026-10-06/, `datamods.mjs` + `refs.py`).
//!
//! Such a module is written under [`ASSETS_DIR`] and named from its own
//! text ([`asset_stem`]), never asked of the model.

use oxc_ast::ast::{
    ArrowFunctionBody, AssignmentTarget, BindingPattern, Expression, FunctionBody, Statement,
};

use crate::babel_view::unparen;

/// Where app text assets are written (the layout owns the folder names).
pub use crate::place::layout::ASSETS_DIR;

/// The text a factory function exports when its WHOLE body is
/// `<module>.exports = <text>`, `<module>` being its second parameter.
pub fn exported_text(function: &Expression<'_>) -> Option<String> {
    let (params, assignment) = match unparen(function) {
        Expression::ArrowFunctionExpression(a) => (
            &a.params,
            match &a.body {
                ArrowFunctionBody::FunctionBody(body) => single_expression(body)?,
                expression => expression.as_expression()?,
            },
        ),
        Expression::FunctionExpression(f) => (&f.params, single_expression(f.body.as_ref()?)?),
        _ => return None,
    };
    let BindingPattern::BindingIdentifier(module) = &params.items.get(1)?.pattern else {
        return None;
    };
    let Expression::AssignmentExpression(assign) = unparen(assignment) else {
        return None;
    };
    if assign.operator != oxc_syntax::operator::AssignmentOperator::Assign {
        return None;
    }
    let AssignmentTarget::StaticMemberExpression(target) = &assign.left else {
        return None;
    };
    let target_is_module_exports = target.property.name == "exports"
        && matches!(&target.object, Expression::Identifier(o) if o.name == module.name);
    if !target_is_module_exports {
        return None;
    }
    match unparen(&assign.right) {
        Expression::StringLiteral(s) => Some(s.value.to_string()),
        Expression::TemplateLiteral(t) if t.expressions.is_empty() => Some(
            t.quasis
                .iter()
                .map(|q| q.value.cooked.as_ref().unwrap_or(&q.value.raw).to_string())
                .collect(),
        ),
        _ => None,
    }
}

/// The one expression a block body consists of: exactly one expression
/// statement, no directives.
fn single_expression<'a>(body: &'a FunctionBody<'a>) -> Option<&'a Expression<'a>> {
    if !body.directives.is_empty() {
        return None;
    }
    let [Statement::ExpressionStatement(stmt)] = body.statements.as_slice() else {
        return None;
    };
    Some(&stmt.expression)
}

/// Words a name is better without.
const FILLER_WORDS: [&str; 16] = [
    "a", "an", "the", "and", "or", "of", "to", "in", "on", "at", "by", "for", "with", "is", "are",
    "be",
];

/// How many words, and characters, an asset name keeps.
const MAX_WORDS: usize = 5;
const MAX_CHARS: usize = 40;

/// How many leading lines are searched for a naming line.
const MAX_LINES: usize = 8;

/// One line stripped of comment, heading and docstring markers.
fn strip_markers(line: &str) -> &str {
    const LEADING: [&str; 10] = [
        "///", "//", "/**", "/*", "*", "#", "\"\"\"", "'''", "<!--", "--",
    ];
    const TRAILING: [&str; 4] = ["*/", "-->", "\"\"\"", "'''"];
    let mut l = line.trim();
    loop {
        let before = l;
        for m in LEADING {
            l = l.strip_prefix(m).unwrap_or(l).trim_start();
        }
        for m in TRAILING {
            l = l.strip_suffix(m).unwrap_or(l).trim_end();
        }
        if l == before {
            return l;
        }
    }
}

/// `line` with every `<…>` markup tag removed.
fn without_tags(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut depth = 0usize;
    for c in line.chars() {
        match c {
            '<' => depth += 1,
            '>' if depth > 0 => depth -= 1,
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out
}

/// The line's lead phrase: up to the first sentence break (`.`, `:`, `;`,
/// `,`, `!`, `?` followed by a space or the end; a dash or `(` anywhere),
/// as lowercase ASCII words — filler words dropped, apostrophes joined.
fn lead_words(line: &str) -> Vec<String> {
    let chars: Vec<char> = line.chars().collect();
    let mut end = chars.len();
    for (i, &c) in chars.iter().enumerate() {
        let next_is_break = chars.get(i + 1).is_none_or(|n| n.is_whitespace());
        let breaks = matches!(c, '—' | '–' | '(')
            || (matches!(c, '.' | ':' | ';' | ',' | '!' | '?') && next_is_break);
        if breaks {
            end = i;
            break;
        }
    }
    let phrase: String = chars[..end]
        .iter()
        .filter(|&&c| c != '\'' && c != '’')
        .collect();
    let all: Vec<String> = phrase
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_ascii_lowercase)
        .collect();
    let kept: Vec<String> = all
        .iter()
        .filter(|w| !FILLER_WORDS.contains(&w.as_str()))
        .cloned()
        .collect();
    if kept.is_empty() { all } else { kept }
}

/// Words joined with `-`, at most [`MAX_WORDS`] and [`MAX_CHARS`].
fn kebab(words: &[String]) -> String {
    let mut out = String::new();
    for w in words.iter().take(MAX_WORDS) {
        let extra = if out.is_empty() { w.len() } else { w.len() + 1 };
        if out.len() + extra > MAX_CHARS {
            break;
        }
        if !out.is_empty() {
            out.push('-');
        }
        out.push_str(w);
    }
    out
}

/// The file stem for an app text asset, from its own text: the lead phrase
/// of its first line that says anything — a Markdown heading, a comment or
/// docstring line, or the first words of prose — after a shebang, blank
/// lines and markup-only lines. Validated by the vendor namer's own
/// validator ([`super::vendor_names::accept_vendor_name`]: 3-40 characters
/// of `[a-z0-9._-]`, nothing generic); None when no line yields a name.
pub fn asset_stem(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with("#!"))
        .take(MAX_LINES)
        .map(|l| kebab(&lead_words(&without_tags(strip_markers(l)))))
        .find_map(|name| super::vendor_names::accept_vendor_name(&name))
}

#[cfg(test)]
#[path = "text_assets/text_assets_test.rs"]
mod text_assets_test;
