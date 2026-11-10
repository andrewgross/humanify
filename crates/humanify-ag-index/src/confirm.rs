//! Source confirmation of a unit row (exp213): a row is kept only when the
//! source file its build's sourcemap assigns it to CONFIRMS it — the
//! unit's string literals and static property names appear in that file's
//! text.
//!
//! Why (experiments/210-precision-breakdown §5b): a bundler's map can file
//! one module's code under a neighbouring source. In 35 node-target shards
//! of readable-stream dependents, Bun 1.4.0's map files string_decoder's
//! and safe-buffer's statements under readable-stream's
//! `async_iterator.js` / `from.js`; the merged index then held 22 rows of
//! readable-stream carrying string_decoder's prints, and the matcher's tie
//! went to readable-stream. The map is wrong, the file is not: the file
//! the map names simply does not contain `"utf16le"` or `isEncoding`.
//!
//! The check only REMOVES rows. A unit with too few checkable tokens, or
//! whose file cannot be read, keeps its row as before (counted
//! `unconfirmable`). It reads the map's own sources, so it is the same for
//! every bundler the builder runs.

use std::collections::{BTreeSet, HashSet};

/// A unit is judged only with at least this many checkable tokens; with
/// fewer it is `unconfirmable` and keeps its row.
pub const MIN_CHECKABLE_TOKENS: usize = 3;

/// A judged unit is refused when fewer than this share of its checkable
/// tokens are found in its source text.
pub const MIN_FOUND_SHARE: f64 = 0.5;

/// A word shorter than this says nothing about which file a unit came
/// from (`id`, `fn`, single letters): it is ignored on both sides.
const MIN_WORD_LEN: usize = 3;

/// What the source said about one unit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// Enough of the unit's tokens are in its source file.
    Confirmed,
    /// Its source file lacks most of its tokens: the map misfiled it.
    Refused,
    /// Fewer than [`MIN_CHECKABLE_TOKENS`] checkable tokens: kept.
    NoTokens,
    /// The source file could not be read: kept.
    Unreadable,
}

/// The words of a source file's text (maximal runs of identifier
/// characters, at least [`MIN_WORD_LEN`] long): what a unit's tokens are
/// looked up in. Word lookup, not substring search, so a 9 MB file costs
/// one pass however many units it has.
#[derive(Debug, Default)]
pub struct SourceWords(HashSet<String>);

impl SourceWords {
    /// The words of `text`. A run right after a backslash is an escape's
    /// tail (`\0SLASH`, `\x00name`, `\u0000name`, `\nword`): the word
    /// behind the escape is a word too, since the unit's token is the
    /// cooked string.
    pub fn of(text: &str) -> SourceWords {
        let mut out = HashSet::new();
        let mut rest = text;
        while let Some(at) = rest.find(is_word_char) {
            let escaped = rest[..at].ends_with('\\');
            rest = &rest[at..];
            let end = rest.find(|c: char| !is_word_char(c)).unwrap_or(rest.len());
            let run = &rest[..end];
            rest = &rest[end..];
            if escaped {
                for tail in escape_tails(run) {
                    if tail.len() >= MIN_WORD_LEN {
                        out.insert(tail.to_string());
                    }
                }
            }
            if run.len() >= MIN_WORD_LEN {
                out.insert(run.to_string());
            }
        }
        SourceWords(out)
    }

    fn has(&self, word: &str) -> bool {
        self.0.contains(word)
    }
}

/// What a word run that follows a backslash can be once its escape is
/// cooked: the run without its escape letter or octal digit (`\n`, `\0`),
/// without a `\xHH` and without a `\uHHHH`.
fn escape_tails(run: &str) -> Vec<&str> {
    let mut tails = vec![run.get(1..).unwrap_or_default()];
    if run.starts_with('x') {
        tails.push(run.get(3..).unwrap_or_default());
    }
    if run.starts_with('u') {
        tails.push(run.get(5..).unwrap_or_default());
    }
    tails
}

fn is_word_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '$'
}

fn words(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c: char| !is_word_char(c))
        .filter(|w| w.len() >= MIN_WORD_LEN)
}

/// The checkable words of each of a unit's content tokens
/// (`humanify_ag_core::libindex::units::NamedStatementPrints::tokens`):
/// string / template text (`S:`) and static property names (`P:`) — what
/// a minifier cannot rename and the source file spells. Regexes and free
/// identifiers are not read, nor a build host path the bundler inlined
/// ([`is_build_host_path`]). A token with no word of [`MIN_WORD_LEN`] is
/// not checkable.
fn checkable(tokens: &BTreeSet<String>) -> Vec<Vec<&str>> {
    tokens
        .iter()
        .filter_map(|t| t.strip_prefix("S:").or_else(|| t.strip_prefix("P:")))
        .filter(|t| !is_build_host_path(t))
        .map(|t| words(t).collect::<Vec<_>>())
        .filter(|w| !w.is_empty())
        .collect()
}

/// A string a bundler wrote, not the source: an inlined `__dirname` /
/// `__filename` is the BUILD host's absolute path into the install's
/// `node_modules`.
fn is_build_host_path(token: &str) -> bool {
    token.starts_with('/') && token.contains("/node_modules/")
}

/// The share of a unit's checkable tokens found in `sources` (a token is
/// found when every one of its words is in one of them), and how many
/// tokens were checkable.
pub fn found_share(tokens: &BTreeSet<String>, sources: &[&SourceWords]) -> (f64, usize) {
    let check = checkable(tokens);
    if check.is_empty() {
        return (1.0, 0);
    }
    let found = check
        .iter()
        .filter(|ws| ws.iter().all(|w| sources.iter().any(|s| s.has(w))))
        .count();
    (found as f64 / check.len() as f64, check.len())
}

/// The verdict on one unit: its tokens against the words of the source
/// files its segments credit to its owner (None: a file that could not be
/// read).
pub fn verdict(tokens: &BTreeSet<String>, sources: &[Option<&SourceWords>]) -> Verdict {
    let Some(read) = sources.iter().copied().collect::<Option<Vec<_>>>() else {
        return Verdict::Unreadable;
    };
    let (share, n) = found_share(tokens, &read);
    if n < MIN_CHECKABLE_TOKENS {
        Verdict::NoTokens
    } else if share < MIN_FOUND_SHARE {
        Verdict::Refused
    } else {
        Verdict::Confirmed
    }
}
