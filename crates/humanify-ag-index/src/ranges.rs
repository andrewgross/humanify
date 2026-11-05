//! npm version ranges over STABLE versions (exp184): the one reader of a
//! `package.json` dependency spec for the index spec's planner, which
//! resolves a dependency the way `npm install --before=<date>` does — the
//! highest stable version the range admits among those published by the
//! date.
//!
//! The grammar is node-semver's for the shapes registry metadata holds:
//! `^`, `~` (and `~>`), x-ranges (`1.x`, `1.2`, `*`, empty), the five
//! comparators, hyphen ranges and `||` unions. A bound with a prerelease
//! (`^1.0.0-beta.1`, `<2.0.0-0`) sits just below its release, so stable
//! versions compare against it as node-semver compares them. Prerelease
//! CANDIDATES are never admitted: the index never holds one.
//!
//! A spec that is not a range — a git or tarball URL, a `file:` path, a
//! `workspace:` link, a dist-tag other than `latest` — is not resolvable
//! from the registry and parses to None; `npm:<package>@<range>` aliases
//! resolve the aliased package.

use humanify_ag_core::libindex::format::Semver;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Op {
    Ge,
    Gt,
    Lt,
    Le,
}

/// One comparator: `op` against `bound`; `pre`: the bound carries a
/// prerelease, so it sits just below `bound` itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Comparator {
    op: Op,
    bound: (u64, u64, u64),
    pre: bool,
}

/// A parsed range: a union of comparator sets (each set is an AND).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Range {
    sets: Vec<Vec<Comparator>>,
}

/// A dependency spec the registry can resolve: the package it names (an
/// `npm:` alias's target) and the range.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DepSpec {
    pub package: String,
    pub range: Range,
}

/// `name: spec` from a `dependencies` map → what to resolve, or None.
pub fn dep_spec(name: &str, spec: &str) -> Option<DepSpec> {
    let (package, range) = match spec.strip_prefix("npm:") {
        Some(rest) => match rest.rfind('@') {
            Some(at) if at > 0 => (rest[..at].to_string(), &rest[at + 1..]),
            _ => (rest.to_string(), "*"),
        },
        None => (name.to_string(), spec),
    };
    Some(DepSpec {
        package,
        range: Range::parse(range)?,
    })
}

/// A partial version `1`, `1.2`, `1.2.3`, with `x`/`X`/`*` wildcards
/// ending it early: (the numbers before the first wildcard, has a
/// prerelease). A leading `v` or `=` is allowed.
fn partial(text: &str) -> Option<(Vec<u64>, bool)> {
    let t = text.trim();
    let t = t.strip_prefix('=').unwrap_or(t).trim_start();
    let t = t.strip_prefix('v').unwrap_or(t);
    let t = t.split_once('+').map_or(t, |(core, _)| core);
    let (core, pre) = match t.split_once('-') {
        Some((c, p)) => (c, Some(p)),
        None => (t, None),
    };
    if let Some(p) = pre
        && (p.is_empty()
            || !p
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-'))
    {
        return None;
    }
    let parts: Vec<&str> = core.split('.').collect();
    if parts.is_empty() || parts.len() > 3 {
        return None;
    }
    let mut nums = Vec::new();
    let mut wild = false;
    for p in parts {
        if matches!(p, "x" | "X" | "*") {
            wild = true;
            continue;
        }
        if wild || p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        nums.push(p.parse().ok()?);
    }
    Some((nums, pre.is_some()))
}

fn full(n: &[u64]) -> (u64, u64, u64) {
    (
        n.first().copied().unwrap_or(0),
        n.get(1).copied().unwrap_or(0),
        n.get(2).copied().unwrap_or(0),
    )
}

fn c(op: Op, bound: (u64, u64, u64), pre: bool) -> Comparator {
    Comparator { op, bound, pre }
}

/// The exclusive upper bound one past a partial version's last number:
/// `1` → `<2.0.0-0`, `1.2` → `<1.3.0-0`.
fn next_after(n: &[u64]) -> Option<Comparator> {
    match n.len() {
        1 => Some(c(Op::Lt, (n[0] + 1, 0, 0), true)),
        2 => Some(c(Op::Lt, (n[0], n[1] + 1, 0), true)),
        _ => None,
    }
}

fn hyphen(lo: &str, hi: &str) -> Option<Vec<Comparator>> {
    let ((ln, lp), (hn, hp)) = (partial(lo)?, partial(hi)?);
    let mut out = Vec::new();
    if !ln.is_empty() {
        out.push(c(Op::Ge, full(&ln), lp));
    }
    if hn.len() == 3 {
        out.push(c(Op::Le, full(&hn), hp));
    } else if let Some(n) = next_after(&hn) {
        out.push(n);
    }
    Some(out)
}

/// One `op` + partial version → its comparators.
fn comparator(op: &str, n: &[u64], pre: bool) -> Vec<Comparator> {
    let f = full(n);
    let mut out = Vec::new();
    match op {
        "" | "=" => {
            if n.len() == 3 {
                out.extend([c(Op::Ge, f, pre), c(Op::Le, f, pre)]);
            } else if !n.is_empty() {
                out.push(c(Op::Ge, f, false));
                out.extend(next_after(n));
            }
        }
        "~" | "~>" => match n.len() {
            0 => {}
            1 => out.extend([c(Op::Ge, f, pre), c(Op::Lt, (n[0] + 1, 0, 0), true)]),
            _ => out.extend([c(Op::Ge, f, pre), c(Op::Lt, (n[0], n[1] + 1, 0), true)]),
        },
        "^" if !n.is_empty() => {
            out.push(c(Op::Ge, f, pre));
            out.push(if n[0] != 0 || n.len() == 1 {
                c(Op::Lt, (n[0] + 1, 0, 0), true)
            } else if n.len() == 2 || n[1] != 0 {
                c(Op::Lt, (0, n[1] + 1, 0), true)
            } else {
                c(Op::Lt, (0, 0, n[2] + 1), true)
            });
        }
        ">=" if !n.is_empty() => out.push(c(Op::Ge, f, pre)),
        ">" => out.push(match n.len() {
            3 => c(Op::Gt, f, pre),
            2 => c(Op::Ge, (n[0], n[1] + 1, 0), false),
            1 => c(Op::Ge, (n[0] + 1, 0, 0), false),
            // `>*` admits nothing
            _ => c(Op::Lt, (0, 0, 0), true),
        }),
        "<" => out.push(match n.len() {
            3 => c(Op::Lt, f, pre),
            0 => c(Op::Lt, (0, 0, 0), true),
            _ => c(Op::Lt, f, true),
        }),
        "<=" => match n.len() {
            3 => out.push(c(Op::Le, f, pre)),
            _ => out.extend(next_after(n)),
        },
        _ => {}
    }
    out
}

const OPS: [&str; 7] = [">=", "<=", "~>", ">", "<", "=", "~"];

fn split_op(tok: &str) -> (&str, &str) {
    if let Some(rest) = tok.strip_prefix('^') {
        return ("^", rest);
    }
    for op in OPS {
        if let Some(rest) = tok.strip_prefix(op) {
            return (op, rest);
        }
    }
    ("", tok)
}

/// One comparator set (no `||`).
fn comparator_set(text: &str) -> Option<Vec<Comparator>> {
    let t = text.trim();
    if matches!(t, "" | "*" | "x" | "X" | "latest") {
        return Some(Vec::new());
    }
    let words: Vec<&str> = t.split_whitespace().collect();
    if words.len() == 3 && words[1] == "-" {
        return hyphen(words[0], words[2]);
    }
    // `>= 1.2.3`: an operator standing alone binds to the next word
    let mut tokens: Vec<String> = Vec::new();
    let mut pending: Option<&str> = None;
    for w in words {
        if w == "^" || OPS.contains(&w) {
            pending = Some(w);
            continue;
        }
        tokens.push(match pending.take() {
            Some(op) => format!("{op}{w}"),
            None => w.to_string(),
        });
    }
    if pending.is_some() {
        return None;
    }
    let mut out = Vec::new();
    for tok in &tokens {
        let (op, rest) = split_op(tok);
        let (n, pre) = partial(rest)?;
        out.extend(comparator(op, &n, pre));
    }
    Some(out)
}

impl Range {
    /// A range, or None when the text is not one.
    pub fn parse(text: &str) -> Option<Range> {
        let sets = text
            .split("||")
            .map(comparator_set)
            .collect::<Option<Vec<_>>>()?;
        Some(Range { sets })
    }

    /// Whether the STABLE version `v` is in the range (a prerelease never
    /// is: the index holds none).
    pub fn satisfies(&self, v: &Semver) -> bool {
        if v.is_prerelease() {
            return false;
        }
        let t = (v.major, v.minor, v.patch);
        self.sets.iter().any(|set| {
            set.iter().all(|cmp| {
                // a prerelease bound sits just below its release
                let ord = t.cmp(&cmp.bound).then(if cmp.pre {
                    std::cmp::Ordering::Greater
                } else {
                    std::cmp::Ordering::Equal
                });
                match cmp.op {
                    Op::Ge => ord.is_ge(),
                    Op::Gt => ord.is_gt(),
                    Op::Lt => ord.is_lt(),
                    Op::Le => ord.is_le(),
                }
            })
        })
    }
}
