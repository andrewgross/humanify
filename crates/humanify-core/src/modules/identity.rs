//! Declared library identity (finding #95): what a vendored module SAYS it
//! is, read from what its bundler kept and what its package wrote — never
//! guessed.
//!
//! Two kinds of knowledge, two kinds of piece (Andrew, 2026-10-10):
//!
//! - **What the bundler kept about each module** is bundler-specific: the
//!   toolchain's `ModuleIdentitySource` piece (P17, chosen once from
//!   detection), whose implementations live in [`provenance`]. Bun keeps
//!   `/*!` legal comments in place and inlines the absolute paths a module
//!   asked for (`__dirname`, `require.resolve`, `import.meta.url`);
//!   esbuild keeps each module's source path as its wrapper key (unminified
//!   builds) and moves legal comments to an end-of-file block keyed by
//!   package path. The piece hands over a [`ModuleProvenance`] — nothing
//!   below it names a bundler.
//! - **What a package says about itself** is bundler-independent: the
//!   readers in [`readers`], each a small shape-checked function over one
//!   module's text (an inlined `package.json`, a `pkg vX` banner, a
//!   highlight.js language definition's `name`, a scoped package quoting
//!   itself in an error message). Package-specific readers (highlight.js)
//!   are entries in the same list, self-gated by their shape.
//!
//! The FIRST reader in [`readers::IDENTITY_READERS`] that names a package
//! or a member wins; a later reader agreeing on the package may only add
//! the version. The result, with its reader and evidence, is the module's
//! [`DeclaredIdentity`], written into the vendor record. Facts too weak to
//! name a module (a repository URL, a bare version constant) are HINTS: the
//! model is shown them, nothing is named by them.
//!
//! Uses: naming (a declared identity names a module before the model is
//! asked; `package/member` files for a member — `highlight.js/actionscript`)
//! and cross-version pairing ([`pair_by_declared_identity`]: equal and
//! unique keys on both sides — a declared identity only ADDS pairs, it
//! never vetoes one).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::vendor_names::{sanitize_fs_name, sanitize_fs_path, strip_js_extension};
use super::{FactoryRecord, NameSource};

pub mod provenance;
pub mod readers;

#[cfg(test)]
#[path = "identity/identity_test.rs"]
mod identity_test;
#[cfg(test)]
#[path = "identity/readers_test.rs"]
mod readers_test;

/// How a bundler wrote a path into the bundle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathKind {
    /// The module's OWN source file (esbuild's wrapper key).
    ModuleFile,
    /// An absolute path the module's code asked for, inlined by the
    /// bundler (`__dirname`, `__filename`, `require.resolve`,
    /// `import.meta.url`) — names the package, not the module's file.
    Inlined,
}

/// A path the bundler wrote for one module.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeclaredPath {
    pub kind: PathKind,
    pub path: String,
}

/// What the bundler preserved about ONE module — the bundler piece's
/// output, the readers' input.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ModuleProvenance {
    /// Legal comments the bundler kept for this module: each comment's
    /// text between its delimiters (a `/*!` comment's leading `!` kept).
    pub kept_comments: Vec<String>,
    pub paths: Vec<DeclaredPath>,
}

/// One entry of a bundle-level legal-comment block (esbuild's default
/// `/*! Bundled license information: … */`): the `package/file` it is keyed
/// by and one comment's text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LegalComment {
    pub path: String,
    pub text: String,
}

/// What the bundler preserved about the WHOLE bundle.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BundleFacts {
    pub legal_comments: Vec<LegalComment>,
}

/// A module's declared identity, as the vendor record writes it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeclaredIdentity {
    /// The reader that declared it ([`readers::IDENTITY_READERS`]).
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// What the module is WITHIN its package: a highlight.js language
    /// (`ActionScript`), a file (`lib/index.js`, `package.json`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub member: Option<String>,
    /// The member's declared aliases (tie-breaks between equal members).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    /// The text it was read from (truncated).
    pub evidence: String,
}

/// One module's declarations: the identity (if any reader named it) and
/// the hints too weak to name it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Declared {
    pub identity: Option<DeclaredIdentity>,
    pub hints: Vec<String>,
}

impl DeclaredIdentity {
    /// The cross-version key: package and member, case-folded (a
    /// declared name can be re-cased between releases — `Javascript` →
    /// `JavaScript`). The version is NOT in it: a bump must not break a
    /// pair.
    pub fn key(&self) -> String {
        format!(
            "{}\u{1}{}",
            self.package.as_deref().unwrap_or("").to_lowercase(),
            self.member.as_deref().unwrap_or("").to_lowercase()
        )
    }

    /// The key with the aliases — the tie-break when two modules declare
    /// the same member (highlight.js 10.x's `c` and `cpp` both say `C++`).
    pub fn alias_key(&self) -> String {
        let mut aliases: Vec<String> = self.aliases.iter().map(|a| a.to_lowercase()).collect();
        aliases.sort();
        format!("{}\u{1}{}", self.key(), aliases.join(","))
    }

    /// The module's name: its package, else its member's file stem.
    pub fn name(&self) -> Option<String> {
        self.package
            .clone()
            .or_else(|| self.member.as_deref().and_then(member_stem))
    }

    /// The record's file for a module NAMED `name` that declares a member
    /// of that package: `(<package folder>, <member stem>)`. None when the
    /// name is not the package (a carried or model name keeps its own
    /// file) or no member is declared.
    pub fn member_file(&self, name: &str) -> Option<(String, String)> {
        let package = self.package.as_deref()?;
        if package != name {
            return None;
        }
        let stem = member_stem(self.member.as_deref()?)?;
        Some((sanitize_fs_path(&strip_js_extension(package)), stem))
    }
}

/// A member's file stem: a FILE member (`lib/index.js`, `package.json`)
/// keeps its path, minus a JavaScript extension; a declared NAME
/// (`ActionScript`, `C++`, `G-code (ISO 6983)`) is slugged (`actionscript`,
/// `cpp`, `g-code-iso-6983`). Short declared names stay (`go`, `c`, `d`).
pub fn member_stem(member: &str) -> Option<String> {
    let lower = member.to_lowercase();
    // A FILE member ends in a source extension; anything else is a declared
    // name, even with a slash in it (`C/AL`).
    let is_file = SOURCE_EXTENSIONS.iter().any(|ext| lower.ends_with(ext));
    let stem = if is_file {
        let cut = [".js", ".cjs", ".mjs"]
            .iter()
            .find(|ext| lower.ends_with(*ext))
            .map_or(member.len(), |ext| member.len() - ext.len());
        sanitize_fs_path(&member[..cut])
    } else {
        slug(member)
    };
    (!stem.is_empty()).then_some(stem)
}

/// The extensions a module's own source file has (esbuild's wrapper key).
const SOURCE_EXTENSIONS: [&str; 10] = [
    ".js", ".cjs", ".mjs", ".json", ".ts", ".cts", ".mts", ".jsx", ".tsx", ".node",
];

/// A Latin letter with an accent, as its plain letter (`é` → `e`), so a
/// declared name keeps its word (`Caché` → `cache`).
fn plain_letter(c: char) -> Option<char> {
    const FOLDS: [(&str, char); 7] = [
        ("àáâãäåā", 'a'),
        ("çć", 'c'),
        ("èéêëē", 'e'),
        ("ìíîïī", 'i'),
        ("ñń", 'n'),
        ("òóôõöøō", 'o'),
        ("ùúûüū", 'u'),
    ];
    let lower = c.to_lowercase().next()?;
    FOLDS
        .iter()
        .find(|(set, _)| set.contains(lower))
        .map(|(_, plain)| *plain)
}

/// A declared name as a file stem: lowercase, `+` → `p` (`C++` → `cpp`),
/// `#` → `sharp`, accents dropped, every other run of non-alphanumerics →
/// one `-`.
fn slug(name: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for c in name.chars() {
        let piece = match c {
            '+' => Some("p".to_string()),
            '#' => Some("sharp".to_string()),
            c if c.is_ascii_alphanumeric() => Some(c.to_ascii_lowercase().to_string()),
            c => plain_letter(c).map(String::from),
        };
        match piece {
            Some(p) => {
                if dash && !out.is_empty() {
                    out.push('-');
                }
                dash = false;
                out.push_str(&p);
            }
            None => dash = true,
        }
    }
    sanitize_fs_name(&out)
}

/// Run every reader over one module: the first identity reader to name a
/// package or member wins; a later one naming the SAME package may add the
/// version. Hints are every hint reader's answer.
pub fn declare(body: &str, provenance: &ModuleProvenance) -> Declared {
    let tokens = super::lex::lex_raw(body);
    let view = readers::ModuleView {
        body,
        tokens: &tokens,
        provenance,
    };
    let mut identity: Option<DeclaredIdentity> = None;
    for reader in readers::IDENTITY_READERS {
        let Some(claim) = (reader.read)(&view) else {
            continue;
        };
        match &mut identity {
            None => {
                identity = Some(DeclaredIdentity {
                    source: reader.name.to_string(),
                    package: claim.package,
                    version: claim.version,
                    member: claim.member,
                    aliases: claim.aliases,
                    evidence: claim.evidence,
                });
            }
            Some(id) => {
                let same_package = id.package.is_some()
                    && id.package.as_deref().map(str::to_lowercase)
                        == claim.package.as_deref().map(str::to_lowercase);
                if same_package && id.version.is_none() {
                    id.version = claim.version;
                }
            }
        }
    }
    let hints = readers::HINT_READERS
        .iter()
        .filter_map(|hint| hint(&view))
        .collect();
    Declared { identity, hints }
}

/// One declared pair: indices into the fresh and prior lists, and the key
/// they share.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeclaredPair {
    pub fresh: usize,
    pub prior: usize,
    pub key: String,
}

/// Pair fresh and prior modules whose declared key is EQUAL and UNIQUE on
/// both sides; then, among those left, the same with the aliases in the
/// key. A module without a declared identity, or whose key is shared, is
/// left to the content pairing. Deterministic: in fresh order.
pub fn pair_by_declared_identity(
    fresh: &[Option<&DeclaredIdentity>],
    prior: &[Option<&DeclaredIdentity>],
) -> Vec<DeclaredPair> {
    let mut fresh_done = vec![false; fresh.len()];
    let mut prior_done = vec![false; prior.len()];
    let mut pairs = Vec::new();
    let passes: [fn(&DeclaredIdentity) -> String; 2] =
        [DeclaredIdentity::key, DeclaredIdentity::alias_key];
    for key_of in passes {
        let index = |side: &[Option<&DeclaredIdentity>], done: &[bool]| {
            let mut by_key: HashMap<String, Vec<usize>> = HashMap::new();
            for (i, id) in side.iter().enumerate() {
                if let Some(id) = id
                    && !done[i]
                {
                    by_key.entry(key_of(id)).or_default().push(i);
                }
            }
            by_key
        };
        let fresh_keys = index(fresh, &fresh_done);
        let prior_keys = index(prior, &prior_done);
        let mut found: Vec<DeclaredPair> = fresh_keys
            .iter()
            .filter_map(|(key, f)| {
                let p = prior_keys.get(key)?;
                (f.len() == 1 && p.len() == 1).then(|| DeclaredPair {
                    fresh: f[0],
                    prior: p[0],
                    key: key.replace('\u{1}', "/"),
                })
            })
            .collect();
        found.sort_by_key(|p| p.fresh);
        for pair in &found {
            fresh_done[pair.fresh] = true;
            prior_done[pair.prior] = true;
        }
        pairs.extend(found);
    }
    pairs.sort_by_key(|p| p.fresh);
    pairs
}

/// Name every module still on the fallback from its declared identity
/// (before the model is asked). Returns how many were named.
pub fn name_from_declared_identity(factories: &mut [FactoryRecord]) -> usize {
    let mut named = 0;
    for f in factories.iter_mut() {
        if f.name_source != Some(NameSource::Fallback) {
            continue;
        }
        let Some(name) = f.identity.as_ref().and_then(DeclaredIdentity::name) else {
            continue;
        };
        f.name = Some(name);
        f.name_source = Some(NameSource::Declared);
        f.name_origin = Some(NameSource::Declared);
        named += 1;
    }
    named
}
