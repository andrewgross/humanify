//! Which asked identifier does an answer key belong to — the ONE owner
//! (docs/responsibility.md; finding #85).
//!
//! The model answers a JSON object keyed by the identifiers it was asked
//! about. It usually copies the keys verbatim, but not always: in the
//! 2026-10-05 census over five labels' logs (40 runs, ~1.2M answers) it
//! keyed `y$_` as `y$` or `y$$_` 185 times — every one a Bun mint ending
//! in `$_`, in the module lanes, their retries and the coverage sweep. A
//! site that looks the asked id up verbatim reads those answers as
//! "declined"; the sweep then never asked again (2.1.86: 6 of 54
//! leftovers).
//!
//! The rule, per ask (never across asks):
//!
//! 1. A key that IS an asked identifier is that identifier's answer.
//! 2. Any other key may belong to an asked identifier the answer left
//!    unanswered when the two are equal after [`normalize`] (whitespace
//!    trimmed, every run of `$`/`_` read as one separator) — and only when
//!    that reading is UNAMBIGUOUS: exactly one unanswered asked id
//!    normalises to the key, exactly one key normalises to that id, the
//!    normal form has a letter or digit in it (`_` vs `$` alone is never a
//!    match), and the key is not itself a name the program uses (the
//!    caller's `is_other_name` — the model may be naming a binding it was
//!    not asked about). Each such match is reported ([`KeyedAnswer::tolerant`])
//!    so the site records it on the trail.
//! 3. Every other key is STRAY ([`KeyedAnswer::stray`]): it belongs to no
//!    asked identifier. A site that re-asks a missing identifier discloses
//!    the stray keys (`RenameFailures::stray_keys`) — the model is told
//!    which keys it must use — rather than reading the miss as a decline.

use humanify_model::llm::Renames;

/// An answer re-keyed onto the asked identifiers.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct KeyedAnswer {
    /// The answer with every tolerantly matched key rewritten to its asked
    /// identifier (entry order kept); stray entries are left as they came.
    pub renames: Renames,
    /// `(asked identifier, the model's key)` for every tolerant match.
    pub tolerant: Vec<(String, String)>,
    /// Keys that belong to no asked identifier, in answer order.
    pub stray: Vec<String>,
}

impl KeyedAnswer {
    /// The model's own key for `id` when it was matched tolerantly.
    pub fn answer_key(&self, id: &str) -> Option<&str> {
        self.tolerant
            .iter()
            .find(|(asked, _)| asked == id)
            .map(|(_, key)| key.as_str())
    }

    /// Whether the (re-keyed) answer has an entry for `id` at all.
    pub fn answered(&self, id: &str) -> bool {
        self.renames.entries().iter().any(|(k, _)| k == id)
    }
}

/// A key's normal form: trimmed, every maximal run of `$`/`_` collapsed to
/// one `$`. None when nothing but separators is left — `_` and `$` are
/// distinct names, never one name misspelled.
pub fn normalize(key: &str) -> Option<String> {
    let mut out = String::with_capacity(key.len());
    let mut in_run = false;
    for ch in key.trim().chars() {
        if ch == '$' || ch == '_' {
            if !in_run {
                out.push('$');
            }
            in_run = true;
        } else {
            out.push(ch);
            in_run = false;
        }
    }
    out.chars().any(char::is_alphanumeric).then_some(out)
}

/// Re-key `answer` onto `asked` (the rule in the module doc).
/// `is_other_name(key)` says whether the key is a name the program already
/// uses for some binding — such a key is never read as a misspelling.
pub fn key_answer(
    answer: &Renames,
    asked: &[String],
    is_other_name: &dyn Fn(&str) -> bool,
) -> KeyedAnswer {
    let entries = answer.entries();
    let exact = |k: &str| asked.iter().any(|a| a == k);
    let open: Vec<&String> = asked
        .iter()
        .filter(|a| !entries.iter().any(|(k, _)| k == *a))
        .collect();
    // Each non-exact key's single candidate (None = stray).
    let candidate: Vec<Option<&String>> = entries
        .iter()
        .map(|(k, _)| {
            if exact(k) || is_other_name(k) {
                return None;
            }
            let norm = normalize(k)?;
            let mut hits = open
                .iter()
                .filter(|a| normalize(a).as_deref() == Some(norm.as_str()));
            match (hits.next(), hits.next()) {
                (Some(a), None) => Some(*a),
                _ => None,
            }
        })
        .collect();
    let claims = |id: &String| candidate.iter().flatten().filter(|c| **c == id).count();
    let mut out = KeyedAnswer::default();
    let mut rekeyed = Vec::with_capacity(entries.len());
    for ((k, v), cand) in entries.iter().zip(&candidate) {
        match cand {
            Some(id) if claims(id) == 1 => {
                out.tolerant.push(((*id).clone(), k.clone()));
                rekeyed.push(((*id).clone(), v.clone()));
            }
            _ => {
                if !exact(k) {
                    out.stray.push(k.clone());
                }
                rekeyed.push((k.clone(), v.clone()));
            }
        }
    }
    out.renames = Renames::from_entries(rekeyed);
    out
}

#[cfg(test)]
mod answer_keys_test;
