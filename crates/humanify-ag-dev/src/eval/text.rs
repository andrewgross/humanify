//! Text helpers the scorer's line accounting shares (TS: `034/diff-ledger.ts`
//! `maskedHead` / `tokenSet` / `editedLineCounts`, `037/diff-composition.ts`
//! `lcsLen` / `onLcs`, the trees' `.js` walks). One owner each; the
//! composition, the diff ledger and the real-column ledger all read these.

use std::collections::{HashMap, HashSet};
use std::path::Path;

/// The identifier-ish words of a text (`/[A-Za-z_$][\w$]*/g`, ASCII), with
/// their byte offsets.
pub fn words(text: &str) -> Vec<(usize, &str)> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c.is_ascii_alphabetic() || c == b'_' || c == b'$' {
            let start = i;
            i += 1;
            while i < bytes.len()
                && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' || bytes[i] == b'$')
            {
                i += 1;
            }
            out.push((start, &text[start..i]));
        } else {
            i += 1;
        }
    }
    out
}

/// The first line of a text (`text.split("\n", 1)[0]`).
pub fn first_line(text: &str) -> &str {
    text.split('\n').next().unwrap_or("")
}

/// Rename-blind first line: every word replaced by `_` — the bucket key
/// for pairing a novel statement with the removed one it was EDITED from.
pub fn masked_head(text: &str) -> String {
    let line = first_line(text);
    let mut out = String::with_capacity(line.len());
    let mut last = 0;
    for (at, w) in words(line) {
        out.push_str(&line[last..at]);
        out.push('_');
        last = at + w.len();
    }
    out.push_str(&line[last..]);
    out
}

/// Identifier-ish words longer than 2 characters — the overlap score's
/// tokens (edited version vs unrelated statement).
pub fn token_set(text: &str) -> HashSet<&str> {
    words(text)
        .into_iter()
        .map(|(_, w)| w)
        .filter(|w| w.len() > 2)
        .collect()
}

/// `|A ∩ B| / max(|A|, |B|, 1)`.
pub fn overlap(a: &HashSet<&str>, b: &HashSet<&str>) -> f64 {
    let inter = a.iter().filter(|w| b.contains(*w)).count();
    inter as f64 / a.len().max(b.len()).max(1) as f64
}

/// The overlap an edited version must clear to pair with what it was
/// edited from.
pub const EDIT_OVERLAP: f64 = 0.5;

/// Pairs a hash-flipped statement with the removed statement it was
/// EDITED from (TS: diff-composition's tier 3 and diff-ledger's modified
/// pairs, one rule): same rename-blind head, the best token overlap among
/// the candidates not yet used, at least [`EDIT_OVERLAP`].
pub struct EditPairer<'t> {
    texts: Vec<&'t str>,
    by_head: HashMap<String, Vec<usize>>,
    used: HashSet<usize>,
}

impl<'t> EditPairer<'t> {
    /// Over the removed statements' texts (indexes are positions here).
    pub fn new(removed: Vec<&'t str>) -> EditPairer<'t> {
        let mut by_head: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, t) in removed.iter().enumerate() {
            by_head.entry(masked_head(t)).or_default().push(i);
        }
        EditPairer {
            texts: removed,
            by_head,
            used: HashSet::new(),
        }
    }

    /// The removed statement `fresh` is an edit of (consumed), if any.
    pub fn take(&mut self, fresh: &str) -> Option<usize> {
        let sw = token_set(fresh);
        let mut best: Option<(usize, f64)> = None;
        for &c in self.by_head.get(&masked_head(fresh)).into_iter().flatten() {
            if self.used.contains(&c) {
                continue;
            }
            let score = overlap(&sw, &token_set(self.texts[c]));
            if score > best.map_or(0.0, |b| b.1) {
                best = Some((c, score));
            }
        }
        let (c, score) = best?;
        (score >= EDIT_OVERLAP).then(|| {
            self.used.insert(c);
            c
        })
    }

    /// Whether removed statement `i` was taken.
    pub fn used(&self, i: usize) -> bool {
        self.used.contains(&i)
    }
}

/// Set-based changed-line counts of an edited statement pair, both sides:
/// a fresh line counts when it appears nowhere in the prior text, and vice
/// versa — what a line diff would roughly print for the pair.
pub fn edited_line_counts(fresh: &str, prior: &str) -> (usize, usize) {
    let fresh_lines: Vec<&str> = fresh.split('\n').collect();
    let prior_lines: Vec<&str> = prior.split('\n').collect();
    let prior_set: HashSet<&str> = prior_lines.iter().copied().collect();
    let fresh_set: HashSet<&str> = fresh_lines.iter().copied().collect();
    let f = fresh_lines
        .iter()
        .filter(|l| !prior_set.contains(*l))
        .count();
    let p = prior_lines
        .iter()
        .filter(|l| !fresh_set.contains(*l))
        .count();
    (f, p)
}

/// Above this many cells the LCS falls back (a guard on pathological
/// sizes, as the TS did).
const LCS_CELLS: usize = 25_000_000;

/// Length of the longest common subsequence of two line lists (rolling
/// DP); past [`LCS_CELLS`], the multiset intersection (a lower bound).
pub fn lcs_len(a: &[&str], b: &[&str]) -> usize {
    if a.is_empty() || b.is_empty() {
        return 0;
    }
    if a.len() * b.len() > LCS_CELLS {
        let mut counts: HashMap<&str, usize> = HashMap::new();
        for l in b {
            *counts.entry(l).or_default() += 1;
        }
        let mut common = 0;
        for l in a {
            if let Some(n) = counts.get_mut(l)
                && *n > 0
            {
                *n -= 1;
                common += 1;
            }
        }
        return common;
    }
    let mut prev = vec![0usize; b.len() + 1];
    let mut cur = vec![0usize; b.len() + 1];
    for x in a {
        for (j, y) in b.iter().enumerate() {
            cur[j + 1] = if x == y {
                prev[j] + 1
            } else {
                prev[j + 1].max(cur[j])
            };
        }
        std::mem::swap(&mut prev, &mut cur);
        cur.iter_mut().for_each(|c| *c = 0);
    }
    prev[b.len()]
}

/// git-style churn between two line lists: added + deleted lines.
pub fn line_churn(a: &[&str], b: &[&str]) -> usize {
    a.len() + b.len() - 2 * lcs_len(a, b)
}

/// Indices of `fresh` on the LCS of two key sequences (the order-stable
/// ones). Past [`LCS_CELLS`] (or with an empty side) every index counts as
/// in order.
pub fn on_lcs(prior: &[&str], fresh: &[&str]) -> HashSet<usize> {
    let (n, m) = (prior.len(), fresh.len());
    if n == 0 || m == 0 || n * m > LCS_CELLS {
        return (0..m).collect();
    }
    let w = m + 1;
    let mut dp = vec![0u32; (n + 1) * w];
    for i in 1..=n {
        for j in 1..=m {
            dp[i * w + j] = if prior[i - 1] == fresh[j - 1] {
                dp[(i - 1) * w + j - 1] + 1
            } else {
                dp[(i - 1) * w + j].max(dp[i * w + j - 1])
            };
        }
    }
    let mut keep = HashSet::new();
    let (mut i, mut j) = (n, m);
    while i > 0 && j > 0 {
        if prior[i - 1] == fresh[j - 1] {
            keep.insert(j - 1);
            i -= 1;
            j -= 1;
        } else if dp[(i - 1) * w + j] >= dp[i * w + j - 1] {
            i -= 1;
        } else {
            j -= 1;
        }
    }
    keep
}

/// Every `.js` file under `dir`, relative to it, sorted (`/`-separated).
/// A missing directory has none.
pub fn list_js(dir: &Path) -> Result<Vec<String>, String> {
    fn walk(dir: &Path, base: &Path, out: &mut Vec<String>) -> Result<(), String> {
        let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        for entry in entries {
            let entry = entry.map_err(|e| format!("{}: {e}", dir.display()))?;
            let path = entry.path();
            let kind = entry
                .file_type()
                .map_err(|e| format!("{}: {e}", path.display()))?;
            if kind.is_dir() {
                walk(&path, base, out)?;
            } else if path.extension().is_some_and(|x| x == "js") {
                let rel = path.strip_prefix(base).map_err(|e| e.to_string())?;
                out.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    if dir.is_dir() {
        walk(dir, dir, &mut out)?;
    }
    out.sort();
    Ok(out)
}

/// Read a file as UTF-8 text, naming the path on failure.
pub fn read(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))
}

/// Read a file, or None when it does not exist.
pub fn read_opt(path: &Path) -> Result<Option<String>, String> {
    if path.exists() {
        read(path).map(Some)
    } else {
        Ok(None)
    }
}

#[cfg(test)]
#[path = "text_test.rs"]
mod text_test;
