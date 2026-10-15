//! Vendored-copy unit rows (exp115): a row whose FILE, inside its owner's
//! package, is a copy of ANOTHER package — `compiled/<pkg>/`,
//! `lib-vendor/<pkg>/`, `dist/_chunks/libs/<pkg>.mjs`,
//! `build/chunks/<pkg>-<hash>.cjs`. The builder drops rows under a plain
//! `vendor/` directory or behind a bundler module marker already
//! (`humanify-index`'s `sponge_evidence`); what reaches the merge are
//! copies whose path still says so but no marker does. The merge marks
//! them ([`mark_vendored`]) with the package the path names, the unit
//! table carries the mark, and a run decides what a marked row is worth
//! ([`super::lookup::VendoredRows`]).
//!
//! The path grammar, in order (the first that names a package wins):
//!
//! - **direct**: a `node_modules/` directory, or a STRONG vendor directory
//!   (`vendor`, `vendors`, `vendored`, `third_party`, `3rd_party`,
//!   `compiled`, `bundled`, `prebundled`, or a name ending in one after a
//!   `-`, `_` or `.`: `lib-vendor`), followed by the package's directory — or by a
//!   file whose stem the index knows. Marked as it stands.
//! - **confirm**: a WEAK directory (`libs`, `deps`, `external`,
//!   `externals`) followed by a known package, or the file's own stem
//!   (extensions and a bundler chunk hash stripped) naming a known
//!   package. A file name is weak evidence — `events-<hash>.js` is
//!   react-three-fiber's own events module — so the file is marked only
//!   when the named package (or its family) holds at least
//!   [`CONFIRM_MIN`] and [`CONFIRM_SHARE`] of the file's prints itself.
//!
//! A name the owner answers to (its name, its unscoped base), a generic
//! path word ([`GENERIC`]) or a package of the owner's family never counts.

use std::collections::{BTreeMap, HashMap};

use super::format::UnitRow;

/// How a path names its package.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathKind {
    /// A vendor or `node_modules` directory: marked as it stands.
    Direct,
    /// A weak directory or the file's stem: marked once the named package
    /// is shown to hold the file's code.
    Confirm,
}

/// A confirmed file shares at least this many prints with the package it
/// names ...
pub const CONFIRM_MIN: usize = 2;
/// ... and at least this share of its prints.
pub const CONFIRM_SHARE: f64 = 0.1;

const STRONG: [&str; 12] = [
    "vendor",
    "vendors",
    "vendored",
    "third_party",
    "third-party",
    "thirdparty",
    "3rd_party",
    "3rd-party",
    "3rdparty",
    "compiled",
    "bundled",
    "prebundled",
];
const WEAK: [&str; 4] = ["libs", "deps", "external", "externals"];
/// Path words that are never a package's name here.
pub const GENERIC: [&str; 22] = [
    "index", "main", "src", "lib", "dist", "cjs", "esm", "es", "umd", "build", "utils", "util",
    "common", "shared", "types", "chunks", "chunk", "_chunks", "node", "browser", "module", "core",
];
/// File-name suffixes that are a build's flavour, not its name.
const EXTENSIONS: [&str; 14] = [
    "min",
    "prod",
    "production",
    "development",
    "dev",
    "umd",
    "esm",
    "cjs",
    "mjs",
    "js",
    "node",
    "bundle",
    "browser",
    "global",
];

/// A strong vendor directory: one of [`STRONG`], or a name ending in one
/// after a `-`, `_` or `.` (`lib-vendor`).
fn is_strong(dir: &str) -> bool {
    let d = dir.to_ascii_lowercase();
    STRONG.contains(&d.as_str())
        || d.char_indices()
            .filter(|&(_, c)| matches!(c, '-' | '_' | '.'))
            .any(|(i, _)| STRONG.contains(&&d[i + 1..]))
}

/// A bundler's content hash: 8 base64url characters, or 8-20 lowercase
/// hex digits, holding a digit or an upper-case letter (an all-lowercase
/// word is part of the name: `lodash.debounce`).
fn is_hash(s: &str) -> bool {
    let b64 = s.len() == 8
        && s.bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-');
    let hex =
        (8..=20).contains(&s.len()) && s.bytes().all(|c| matches!(c, b'0'..=b'9' | b'a'..=b'f'));
    (b64 || hex)
        && s.bytes()
            .any(|c| c.is_ascii_digit() || c.is_ascii_uppercase())
}

/// A file name's stem: its flavour extensions and a trailing chunk hash
/// stripped (`typescript-DKNvXqx9.cjs` → `typescript`).
pub fn file_stem(name: &str) -> &str {
    let mut s = name;
    while let Some((head, ext)) = s.rsplit_once('.') {
        if head.is_empty() || !EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str()) {
            break;
        }
        s = head;
    }
    for sep in ['-', '.'] {
        if let Some((head, tail)) = s.rsplit_once(sep) {
            // a base64url hash may itself hold a '-': try the 8 last bytes
            if !head.is_empty() && is_hash(tail) {
                return head;
            }
        }
    }
    if s.len() > 9 {
        let (head, tail) = s.split_at(s.len() - 8);
        if let Some(h) = head.strip_suffix('-').or_else(|| head.strip_suffix('.'))
            && !h.is_empty()
            && is_hash(tail)
        {
            return h;
        }
    }
    s
}

/// The package named at `segs[i]` (two segments when scoped), and whether
/// it is a directory (else the file's stem).
fn package_at(segs: &[&str], i: usize) -> Option<(String, bool)> {
    let s = *segs.get(i)?;
    if s.starts_with('@') && i + 1 < segs.len() {
        let is_dir = i + 1 < segs.len() - 1;
        let base = if is_dir {
            segs[i + 1]
        } else {
            file_stem(segs[i + 1])
        };
        return Some((format!("{s}/{base}"), is_dir));
    }
    let is_dir = i < segs.len() - 1;
    Some(((if is_dir { s } else { file_stem(s) }).to_string(), is_dir))
}

/// The package `file` (a path inside `owner`'s package) is a copy of, and
/// how the path says so; None for the owner's own code. `known`: the
/// package names the index knows; `same_family`: one package family?
pub fn named_package(
    owner: &str,
    file: &str,
    known: &dyn Fn(&str) -> bool,
    same_family: &dyn Fn(&str, &str) -> bool,
) -> Option<(String, PathKind)> {
    let segs: Vec<&str> = file
        .split('/')
        .filter(|s| !s.is_empty() && *s != ".")
        .collect();
    let base = owner
        .split_once('/')
        .filter(|_| owner.starts_with('@'))
        .map(|(_, b)| b);
    let ok = |n: &str| {
        !n.is_empty()
            && n != owner
            && Some(n) != base
            && !GENERIC.contains(&n.to_ascii_lowercase().as_str())
            && !same_family(owner, n)
    };
    for (i, dir) in segs.iter().enumerate().take(segs.len().saturating_sub(1)) {
        let low = dir.to_ascii_lowercase();
        if low == "node_modules" {
            if let Some((n, _)) = package_at(&segs, i + 1)
                && ok(&n)
            {
                return Some((n, PathKind::Direct));
            }
        } else if is_strong(&low) {
            if let Some((n, is_dir)) = package_at(&segs, i + 1)
                && ok(&n)
                && (is_dir || known(&n))
            {
                return Some((n, PathKind::Direct));
            }
        } else if WEAK.contains(&low.as_str())
            && let Some((n, _)) = package_at(&segs, i + 1)
            && ok(&n)
            && known(&n)
        {
            return Some((n, PathKind::Confirm));
        }
    }
    let n = file_stem(segs.last()?);
    (ok(n) && known(n)).then(|| (n.to_string(), PathKind::Confirm))
}

/// What [`mark_vendored`] marked.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct VendoredStats {
    pub rows: usize,
    pub files: usize,
}

/// Mark every row of one slice whose file is a vendored copy
/// (module doc): `row.vendored` = the package its path names. `rows` must
/// be grouped by hash (the merge's (hash, owner, file) order).
pub fn mark_vendored(
    rows: &mut [UnitRow],
    known: &dyn Fn(&str) -> bool,
    same_family: &dyn Fn(&str, &str) -> bool,
) -> VendoredStats {
    // per (owner, file): its named package and kind, once
    let mut named: HashMap<(&str, &str), Option<(String, PathKind)>> = HashMap::new();
    for r in rows.iter() {
        named
            .entry((r.owner.as_str(), r.file.as_str()))
            .or_insert_with(|| named_package(&r.owner, &r.file, known, same_family));
    }
    // per candidate file: (its prints, the prints its named package holds)
    let mut tally: BTreeMap<(&str, &str), (usize, usize)> = BTreeMap::new();
    let mut start = 0;
    while start < rows.len() {
        let end = start
            + rows[start..]
                .iter()
                .position(|r| r.hash != rows[start].hash)
                .unwrap_or(rows.len() - start);
        let group = &rows[start..end];
        for r in group {
            let Some(Some((n, _))) = named.get(&(r.owner.as_str(), r.file.as_str())) else {
                continue;
            };
            let held = group
                .iter()
                .any(|o| o.owner != r.owner && (o.owner == *n || same_family(&o.owner, n)));
            let t = tally
                .entry((r.owner.as_str(), r.file.as_str()))
                .or_default();
            t.0 += 1;
            t.1 += usize::from(held);
        }
        start = end;
    }
    let marked: HashMap<(&str, &str), &str> = tally
        .iter()
        .filter_map(|(&(o, f), &(n_rows, shared))| {
            let (name, kind) = named[&(o, f)].as_ref()?;
            let confirmed = shared >= CONFIRM_MIN && shared as f64 >= CONFIRM_SHARE * n_rows as f64;
            (*kind == PathKind::Direct || confirmed).then_some(((o, f), name.as_str()))
        })
        .collect();
    let stats = VendoredStats {
        files: marked.len(),
        rows: rows
            .iter()
            .filter(|r| marked.contains_key(&(r.owner.as_str(), r.file.as_str())))
            .count(),
    };
    let marks: Vec<Option<String>> = rows
        .iter()
        .map(|r| {
            marked
                .get(&(r.owner.as_str(), r.file.as_str()))
                .map(|n| n.to_string())
        })
        .collect();
    drop(marked);
    drop(tally);
    drop(named);
    for (r, m) in rows.iter_mut().zip(marks) {
        r.vendored = m;
    }
    stats
}
