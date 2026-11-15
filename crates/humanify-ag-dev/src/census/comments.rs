//! `cargo dev census comments [--base REF] [--all] [--root D]`: what a
//! comment or a document may say, as five rules. ADVISORY.
//!
//! A comment says what the code below does, or why it is shaped that way,
//! in terms of things that exist in the tree now. History lives in git and
//! in the experiment write-ups. The rules:
//!
//! - **C1 provenance** (comments in `crates/`): an experiment id (`exp`
//!   and three digits), `experiments/` and three digits, `finding #N`, or
//!   an ISO date.
//! - **C2 deleted file** (comments in `crates/`): a file path or file name
//!   that names no file in the tree. A path resolves when a tracked file
//!   is that path, ends with it, or sits at it relative to the commenting
//!   file; a bare name resolves when some tracked file has that name.
//!   Absolute paths, URLs and `git show <rev>:<path>` (a deliberate
//!   pointer into history) are left alone.
//! - **C3 directive** (comments in `crates/`): one of the directive words
//!   (`has_directive` lists them) without a tag `(enforced: <name>)`, where
//!   the name is a gate stage, a `clippy::` lint or a function in
//!   `crates/` (a test).
//! - **C4 link** (`CLAUDE.md`, `README.md`, `docs/`): a relative Markdown
//!   link, or a `docs/….md` reference, that resolves to nothing.
//! - **C5 Markdown file**: a tracked `.md` file that no pattern of
//!   `dev/census/docs-allowlist.json` allows.
//!
//! Every rule is counted over the whole tree. A finding is NEW when the
//! merge-base of HEAD with `--base` (default `main`) has no finding of the
//! same rule with the same text in the same file, judged against today's
//! tree, so deleting a file does not turn old lines into new findings.
//! Exit 1 (the gate prints REVIEW, not FAIL) when there is a new
//! finding; exit 2 when the tree or the base cannot be read. `--all`
//! lists every finding, the work list for a comment cleanup.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::Path;

use super::clones::scan_root;
use super::concurrency::scan;
use super::rust_fns::{self, Source};
use crate::HostEnv;
use crate::settings::Parsed;

/// One of the five rules.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Rule {
    Provenance,
    DeletedFile,
    Directive,
    Link,
    Markdown,
}

impl Rule {
    pub const ALL: [Rule; 5] = [
        Rule::Provenance,
        Rule::DeletedFile,
        Rule::Directive,
        Rule::Link,
        Rule::Markdown,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Rule::Provenance => "C1",
            Rule::DeletedFile => "C2",
            Rule::Directive => "C3",
            Rule::Link => "C4",
            Rule::Markdown => "C5",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Rule::Provenance => "provenance (experiment id, finding #, date)",
            Rule::DeletedFile => "reference to a file not in the tree",
            Rule::Directive => "never/must/do not without (enforced: <check>)",
            Rule::Link => "Markdown link or docs reference that resolves to nothing",
            Rule::Markdown => "Markdown file outside dev/census/docs-allowlist.json",
        }
    }
}

/// One finding: the rule, where, and the line's text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    pub rule: Rule,
    pub file: String,
    pub line: usize,
    pub text: String,
}

/// What the rules resolve names against: today's tree.
#[derive(Clone, Debug, Default)]
pub struct Context {
    files: BTreeSet<String>,
    names: BTreeSet<String>,
    roots: BTreeSet<String>,
    fns: BTreeSet<String>,
    allowed: Vec<String>,
}

impl Context {
    /// The tree of `files` (repo-relative), the fn names of `rust`, and
    /// the Markdown allowlist's patterns.
    pub fn new(
        files: impl IntoIterator<Item = String>,
        rust: &[Source],
        allowed: Vec<String>,
    ) -> Context {
        let files: BTreeSet<String> = files.into_iter().collect();
        let names = files
            .iter()
            .map(|f| f.rsplit('/').next().unwrap_or(f).to_string())
            .collect();
        // The tree's top-level folders, and the two roots of the deleted
        // TypeScript program: a data file named under one of them is a
        // repository path; elsewhere it is a run's output file.
        let roots = files
            .iter()
            .filter_map(|f| f.split_once('/').map(|(r, _)| r.to_string()))
            .chain(["src".to_string(), "scripts".to_string()])
            .collect();
        let fns = rust.iter().flat_map(|s| fn_names(&s.text)).collect();
        Context {
            files,
            names,
            roots,
            fns,
            allowed,
        }
    }

    /// Whether `token` names a repository file the rule should resolve:
    /// any path or name of a source file (Rust, TypeScript, JavaScript
    /// modules, Python, shell, Markdown), and a data file only by a path
    /// under one of the tree's roots.
    fn names_a_repo_file(&self, token: &str) -> bool {
        let ext = token.rsplit_once('.').map_or("", |(_, e)| e);
        SOURCE_EXTENSIONS.contains(&ext)
            || token
                .split_once('/')
                .is_some_and(|(root, _)| self.roots.contains(root))
    }

    fn resolves(&self, from: &str, token: &str) -> bool {
        if !token.contains('/') {
            return self.names.contains(token);
        }
        let dir = from.rsplit_once('/').map_or("", |(d, _)| d);
        let relative = normalize(&format!("{dir}/{token}"));
        let suffix = format!("/{token}");
        self.files.contains(token)
            || self.files.contains(&relative)
            || self.files.iter().any(|f| f.ends_with(&suffix))
    }

    fn check_exists(&self, name: &str) -> bool {
        crate::gate::check::STAGES.iter().any(|s| s.name == name)
            || name.strip_prefix("clippy::").is_some_and(|l| !l.is_empty())
            || self.fns.contains(name)
    }
}

/// `a/./b/../c` -> `a/c`.
fn normalize(path: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                out.pop();
            }
            p => out.push(p),
        }
    }
    out.join("/")
}

fn is_word(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// Every name after a `fn ` keyword in `text`.
fn fn_names(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find("fn ") {
        let before = rest[..at].chars().next_back();
        let name: String = rest[at + 3..].chars().take_while(|c| is_word(*c)).collect();
        if !before.is_some_and(is_word) && !name.is_empty() {
            out.push(name);
        }
        rest = &rest[at + 3..];
    }
    out
}

/// Whether `s` has, at a word start, `prefix` followed by exactly `n`
/// digits and then no further word character.
fn has_id(s: &str, prefix: &str, n: usize) -> bool {
    s.match_indices(prefix).any(|(at, _)| {
        let start_ok = !s[..at].chars().next_back().is_some_and(is_word);
        let tail: Vec<char> = s[at + prefix.len()..].chars().take(n + 1).collect();
        let digits = tail.iter().take(n).filter(|c| c.is_ascii_digit()).count();
        let after_ok = !tail.get(n).is_some_and(|c| c.is_ascii_digit());
        start_ok && digits == n && after_ok
    })
}

/// An ISO date `20YY-MM-DD` in `s`.
fn has_date(s: &str) -> bool {
    let b = s.as_bytes();
    b.windows(10).enumerate().any(|(i, w)| {
        let shape = w.iter().enumerate().all(|(k, c)| match k {
            4 | 7 => *c == b'-',
            _ => c.is_ascii_digit(),
        });
        let edge = |j: Option<&u8>| !j.is_some_and(|c| c.is_ascii_alphanumeric());
        shape
            && w.starts_with(b"20")
            && edge(i.checked_sub(1).and_then(|j| b.get(j)))
            && edge(b.get(i + 10))
    })
}

fn provenance(line: &str) -> bool {
    let finding = line
        .split("finding #")
        .skip(1)
        .any(|t| t.starts_with(|c: char| c.is_ascii_digit()));
    has_id(line, "exp", 3) || has_id(line, "experiments/", 3) || finding || has_date(line)
}

/// `line` with every `git show <rev>:<path>` and URL blanked.
fn without_history_and_urls(line: &str) -> String {
    let bare = |w: &str| {
        w.trim_matches(|c: char| !c.is_ascii_alphanumeric())
            .to_string()
    };
    let words: Vec<&str> = line.split(' ').collect();
    let kept: Vec<&str> = words
        .iter()
        .enumerate()
        .map(|(i, w)| {
            let after_git_show =
                i >= 2 && bare(words[i - 1]) == "show" && bare(words[i - 2]) == "git";
            if after_git_show || w.contains("://") {
                ""
            } else {
                *w
            }
        })
        .collect();
    kept.join(" ")
}

const SOURCE_EXTENSIONS: &[&str] = &["rs", "md", "ts", "mts", "mjs", "py", "sh"];
const FILE_EXTENSIONS: &[&str] = &[
    "rs", "md", "ts", "mts", "mjs", "js", "py", "sh", "json", "jsonl", "toml", "tsv",
];

/// The file paths and file names `line` names.
fn file_tokens(line: &str) -> Vec<String> {
    let cleaned = without_history_and_urls(line);
    cleaned
        .split(|c: char| !(is_word(c) || matches!(c, '.' | '/' | '-')))
        .filter(|t| !t.starts_with('/') && !t.starts_with('~'))
        .map(|t| t.trim_start_matches("./").trim_end_matches('.'))
        .filter(|t| {
            t.rsplit_once('.').is_some_and(|(stem, ext)| {
                !stem.is_empty() && !stem.ends_with('/') && FILE_EXTENSIONS.contains(&ext)
            })
        })
        .map(str::to_string)
        .collect()
}

/// The words of `line`, lowercased.
fn words(line: &str) -> Vec<String> {
    line.split(|c: char| !c.is_ascii_alphabetic())
        .filter(|w| !w.is_empty())
        .map(str::to_ascii_lowercase)
        .collect()
}

fn has_directive(line: &str) -> bool {
    let w = words(line);
    w.iter().any(|x| x == "never" || x == "must")
        || w.windows(2).any(|p| p[0] == "do" && p[1] == "not")
}

/// The names in the line's `(enforced: <name>)` tags.
fn enforced_tags(line: &str) -> Vec<&str> {
    line.split("(enforced:")
        .skip(1)
        .filter_map(|t| t.split(')').next())
        .map(str::trim)
        .collect()
}

fn directive_ok(line: &str, ctx: &Context) -> bool {
    let tags = enforced_tags(line);
    !tags.is_empty() && tags.iter().all(|t| ctx.check_exists(t))
}

/// C1, C2 and C3 over the comments of one Rust file.
pub fn rust_findings(file: &Source, ctx: &Context) -> Vec<Finding> {
    let mut out = Vec::new();
    for (first, text) in scan::comments(&file.text) {
        for (k, line) in text.lines().enumerate() {
            let at = |rule| Finding {
                rule,
                file: file.rel.clone(),
                line: first + k,
                text: line.trim().to_string(),
            };
            if provenance(line) {
                out.push(at(Rule::Provenance));
            }
            let gone = |t: &String| ctx.names_a_repo_file(t) && !ctx.resolves(&file.rel, t);
            if file_tokens(line).iter().any(gone) {
                out.push(at(Rule::DeletedFile));
            }
            if has_directive(line) && !directive_ok(line, ctx) {
                out.push(at(Rule::Directive));
            }
        }
    }
    out
}

/// The relative link targets of `line` (`[text](target)`), anchors cut.
fn link_targets(line: &str) -> Vec<&str> {
    line.split("](")
        .skip(1)
        .filter_map(|t| t.split(')').next())
        .filter(|t| !t.contains("://") && !t.starts_with('#') && !t.starts_with("mailto:"))
        .map(|t| t.split('#').next().unwrap_or(t))
        .filter(|t| !t.is_empty())
        .collect()
}

/// Whether the rules read this Markdown file's links (the kept docs).
pub fn checked_doc(rel: &str) -> bool {
    rel == "CLAUDE.md" || rel == "README.md" || rel.starts_with("docs/")
}

/// C4 over one Markdown file.
pub fn markdown_findings(file: &Source, ctx: &Context) -> Vec<Finding> {
    let dir = file.rel.rsplit_once('/').map_or("", |(d, _)| d);
    let exists = |rel: &str| {
        ctx.files.contains(rel) || ctx.files.iter().any(|f| f.starts_with(&format!("{rel}/")))
    };
    let mut out = Vec::new();
    for (k, line) in file.text.lines().enumerate() {
        let bad_link = link_targets(line)
            .into_iter()
            .map(|t| normalize(&format!("{dir}/{t}")))
            .filter(|t| !exists(t));
        let bad_ref = file_tokens(line)
            .into_iter()
            .filter(|t| t.starts_with("docs/") && t.ends_with(".md") && !exists(t));
        for _ in bad_link.chain(bad_ref) {
            out.push(Finding {
                rule: Rule::Link,
                file: file.rel.clone(),
                line: k + 1,
                text: line.trim().to_string(),
            });
        }
    }
    out
}

/// `*` matches within one path segment, `**` any number of segments.
pub fn glob_match(pattern: &str, path: &str) -> bool {
    let p: Vec<&str> = pattern.split('/').collect();
    let s: Vec<&str> = path.split('/').collect();
    match_segments(&p, &s)
}

fn match_segments(p: &[&str], s: &[&str]) -> bool {
    match (p.first(), s.first()) {
        (None, None) => true,
        (Some(&"**"), _) => {
            match_segments(&p[1..], s) || (!s.is_empty() && match_segments(p, &s[1..]))
        }
        (Some(a), Some(b)) => match_one(a, b) && match_segments(&p[1..], &s[1..]),
        _ => false,
    }
}

fn match_one(pattern: &str, seg: &str) -> bool {
    match pattern.split_once('*') {
        None => pattern == seg,
        Some((head, tail)) => {
            seg.len() >= head.len() + tail.len() && seg.starts_with(head) && seg.ends_with(tail)
        }
    }
}

/// C5: every Markdown file of `md` that no allowlist pattern allows.
pub fn markdown_file_findings(md: &[String], ctx: &Context) -> Vec<Finding> {
    md.iter()
        .filter(|f| !ctx.allowed.iter().any(|p| glob_match(p, f)))
        .map(|f| Finding {
            rule: Rule::Markdown,
            file: f.clone(),
            line: 0,
            text: String::new(),
        })
        .collect()
}

/// Every finding of a tree: its Rust sources and its Markdown files.
pub fn findings(rust: &[Source], markdown: &[Source], ctx: &Context) -> Vec<Finding> {
    let mut out: Vec<Finding> = rust.iter().flat_map(|s| rust_findings(s, ctx)).collect();
    out.extend(
        markdown
            .iter()
            .filter(|s| checked_doc(&s.rel))
            .flat_map(|s| markdown_findings(s, ctx)),
    );
    let md: Vec<String> = markdown.iter().map(|s| s.rel.clone()).collect();
    out.extend(markdown_file_findings(&md, ctx));
    out
}

type Key<'a> = (Rule, &'a str, &'a str);

/// The findings of `now` that `base` does not have (same rule, file and
/// text; counted, so a second copy of an old line is new).
pub fn new_findings<'a>(now: &'a [Finding], base: &[Finding]) -> Vec<&'a Finding> {
    let mut left: BTreeMap<Key, usize> = BTreeMap::new();
    for f in base {
        *left.entry((f.rule, &f.file, &f.text)).or_default() += 1;
    }
    now.iter()
        .filter(
            |f| match left.get_mut(&(f.rule, f.file.as_str(), f.text.as_str())) {
                Some(n) if *n > 0 => {
                    *n -= 1;
                    false
                }
                _ => true,
            },
        )
        .collect()
}

fn line_of(f: &Finding) -> String {
    let at = if f.line > 0 {
        format!("{}:{}", f.file, f.line)
    } else {
        f.file.clone()
    };
    let text: String = f.text.chars().take(160).collect();
    format!("  {at} [{}] {text}", f.rule.id())
}

/// The report and exit code: totals per rule, the new findings, and
/// with `all` every finding.
pub fn report(now: &[Finding], base: &[Finding], merge_base: &str, all: bool) -> (u8, String) {
    let mut out = String::from(
        "census comments — five rules over crates/ comments and the repository's Markdown (ADVISORY)\n",
    );
    let new = new_findings(now, base);
    let _ = writeln!(out, "\n  rule  {:>6} {:>5}  what", "tree", "new");
    for rule in Rule::ALL {
        let count = |fs: &mut dyn Iterator<Item = &Finding>| fs.filter(|f| f.rule == rule).count();
        let _ = writeln!(
            out,
            "  {}    {:>6} {:>5}  {}",
            rule.id(),
            count(&mut now.iter()),
            count(&mut new.iter().copied()),
            rule.name()
        );
    }
    if all {
        let _ = writeln!(out, "\nEVERY FINDING ({}):", now.len());
        now.iter().for_each(|f| {
            let _ = writeln!(out, "{}", line_of(f));
        });
    }
    let short = &merge_base[..merge_base.len().min(12)];
    if new.is_empty() {
        let _ = writeln!(
            out,
            "\nCOMMENT CENSUS GREEN: no finding the merge-base ({short}) did not have."
        );
        return (0, out);
    }
    let _ = writeln!(
        out,
        "\nNEW since the merge-base ({short}): {}\n{}\n\nCOMMENT CENSUS: REVIEW — ADVISORY. Rewrite the line to describe the code as it is now, add a tag naming the check, or drop it.",
        new.len(),
        new.iter()
            .map(|f| line_of(f))
            .collect::<Vec<_>>()
            .join("\n")
    );
    (1, out)
}

/// The repository's tracked files at `rev` (`None`: the working tree's
/// index, kept only where the file still exists).
fn tracked(root: &Path, rev: Option<&str>) -> Result<Vec<String>, String> {
    let listing = match rev {
        Some(r) => crate::proc::git(root, &["ls-tree", "-r", "--name-only", r])?,
        None => crate::proc::git(root, &["ls-files"])?,
    };
    Ok(listing
        .lines()
        .filter(|f| rev.is_some() || root.join(f).exists())
        .map(str::to_string)
        .collect())
}

fn markdown_sources(
    root: &Path,
    files: &[String],
    rev: Option<&str>,
) -> Result<Vec<Source>, String> {
    let md: Vec<String> = files
        .iter()
        .filter(|f| f.ends_with(".md"))
        .cloned()
        .collect();
    let texts = match rev {
        Some(r) => rust_fns::git_blobs(root, r, &md)?,
        None => md
            .iter()
            .map(|f| std::fs::read(root.join(f)).map(|b| String::from_utf8_lossy(&b).into_owned()))
            .collect::<Result<_, _>>()
            .map_err(|e| format!("reading Markdown: {e}"))?,
    };
    Ok(md
        .into_iter()
        .zip(texts)
        .map(|(rel, text)| Source { rel, text })
        .collect())
}

fn both(root: &Path, base: &str) -> Result<(String, Vec<Finding>, Vec<Finding>), String> {
    let merge_base = crate::proc::git(root, &["merge-base", "HEAD", base])?;
    let merge_base = merge_base.trim().to_string();
    let files = tracked(root, None)?;
    let rust = rust_fns::worktree_sources(root)?;
    let allow = crate::committed::census::docs_allowlist(root)?;
    let ctx = Context::new(files.iter().cloned(), &rust, allow.value.allowed);
    let now = findings(&rust, &markdown_sources(root, &files, None)?, &ctx);
    let base_files = tracked(root, Some(&merge_base))?;
    let base_rust = rust_fns::git_sources(root, &merge_base)?;
    let base_md = markdown_sources(root, &base_files, Some(&merge_base))?;
    let then = findings(&base_rust, &base_md, &ctx);
    Ok((merge_base, now, then))
}

/// Run the verb.
pub fn run(p: &Parsed, _host: &HostEnv) -> (u8, String, String) {
    let base = p.settings.text("dev.base").unwrap_or_else(|| "main".into());
    match both(&scan_root(p), &base) {
        Ok((merge_base, now, then)) => {
            let (code, out) = report(&now, &then, &merge_base, p.settings.flag_on("dev.all"));
            (code, out, String::new())
        }
        Err(e) => (2, String::new(), format!("census comments: {e}\n")),
    }
}

#[cfg(test)]
#[path = "comments_test.rs"]
mod comments_test;
