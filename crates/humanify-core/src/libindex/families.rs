//! Package families (docs/design/library-index.md §7): a claim naming
//! another member of the true package's family is family-correct, not
//! wrong. ONE reader of `families.json`, the data the index ships
//! (`humanify_index::merge` copies [`FAMILIES_JSON`] into every index) and
//! the lookup side reads — library discovery's isolation veto asks it which
//! neighbours are "the same package" (docs/design/library-discovery.md §3b).
//!
//! The LINKS npm registry metadata derives (exp104: one repository, a
//! renamed-to deprecation, a GitHub fork republished under a scope;
//! `families-derived.json`, the ONE file the exp096 scorer and this matcher
//! both read) join two packages PAIRWISE ([`PackageFamilies::same`]): a
//! chain of links counts, a chain through a scope family does not
//! (`@openpgp/tweetnacl` ~ `tweetnacl` and `@openpgp/jsdoc` ~ `jsdoc` must
//! not make `tweetnacl` ~ `jsdoc`).

use std::collections::{BTreeSet, HashMap};

use serde::Deserialize;

/// The shipped families.
pub const FAMILIES_JSON: &str = include_str!("families.json");

/// The shipped metadata-derived links (exp104's `families-derived.json`).
pub const DERIVED_JSON: &str = include_str!("families-derived.json");

#[derive(Deserialize)]
struct DerivedFile {
    links: Vec<DerivedLink>,
}

#[derive(Deserialize)]
struct DerivedLink {
    a: String,
    b: String,
}

#[derive(Deserialize)]
struct FamiliesFile {
    families: Vec<FamilyRow>,
    rules: Rules,
}

#[derive(Deserialize)]
struct FamilyRow {
    name: String,
    members: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Rules {
    scoped_forks: Vec<(String, String)>,
}

/// The families, parsed.
pub struct PackageFamilies {
    families: Vec<FamilyRow>,
    scoped_forks: Vec<(String, String)>,
    /// Union-find over the derived links (package → parent); empty without.
    parent: HashMap<String, String>,
    links: usize,
}

impl PackageFamilies {
    /// The shipped families.
    pub fn shipped() -> PackageFamilies {
        let file: FamiliesFile =
            serde_json::from_str(FAMILIES_JSON).expect("the shipped families.json parses");
        PackageFamilies {
            families: file.families,
            scoped_forks: file.rules.scoped_forks,
            parent: HashMap::new(),
            links: 0,
        }
    }

    /// These families joined by `links` (pairs of package names).
    pub fn with_links(mut self, links: impl IntoIterator<Item = (String, String)>) -> Self {
        for (a, b) in links {
            self.union(&a, &b);
            self.links += 1;
        }
        self
    }

    /// These families joined by the shipped derived links.
    pub fn with_shipped_links(self) -> Self {
        self.with_links(shipped_links())
    }

    /// These families joined by the shipped derived links whose two
    /// packages are both in `names` (a run's index packages: the matcher
    /// reads no link about a package it cannot name).
    pub fn with_shipped_links_among(self, names: &BTreeSet<String>) -> Self {
        self.with_links(
            shipped_links()
                .into_iter()
                .filter(|(a, b)| names.contains(a) && names.contains(b)),
        )
    }

    /// How many links joined these families.
    pub fn link_count(&self) -> usize {
        self.links
    }

    fn find(&self, a: &str) -> String {
        let mut root = a;
        while let Some(p) = self.parent.get(root) {
            if p == root {
                break;
            }
            root = p;
        }
        root.to_string()
    }

    fn union(&mut self, a: &str, b: &str) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra != rb {
            let (lo, hi) = if ra < rb { (ra, rb) } else { (rb, ra) };
            self.parent.insert(hi, lo);
        }
    }

    /// Are `a` and `b` one family: the same [`Self::family`], or joined by
    /// a chain of derived links (pairwise: never through a scope family).
    pub fn same(&self, a: &str, b: &str) -> bool {
        a == b
            || self.family(a) == self.family(b)
            || (!self.parent.is_empty() && self.find(a) == self.find(b))
    }

    /// The family `package` belongs to: a listed family's name (a member
    /// ending in `*` is a prefix), else a scoped fork's original, else the
    /// package's npm scope (`sameScope`), else the package itself.
    pub fn family(&self, package: &str) -> String {
        let listed = self.families.iter().find(|f| {
            f.members.iter().any(|m| match m.strip_suffix('*') {
                Some(prefix) => package.starts_with(prefix),
                None => m == package,
            })
        });
        if let Some(f) = listed {
            return f.name.clone();
        }
        if let Some((original, _)) = self.scoped_forks.iter().find(|(_, fork)| fork == package) {
            return original.clone();
        }
        match package.split_once('/') {
            Some((scope, _)) if scope.starts_with('@') => scope.to_string(),
            _ => package.to_string(),
        }
    }
}

/// The shipped derived links, as pairs.
fn shipped_links() -> Vec<(String, String)> {
    let file: DerivedFile =
        serde_json::from_str(DERIVED_JSON).expect("the shipped families-derived.json parses");
    file.links.into_iter().map(|l| (l.a, l.b)).collect()
}
