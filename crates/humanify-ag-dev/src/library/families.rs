//! The library SCORER's package-family judgement (experiments/096
//! `families_lib.py` over exp095's `libix.family`), kept apart from the
//! matcher's (`humanify_ag_core::libindex::families`) on purpose: the two
//! read the same files but spell a family differently. libix prefixes
//! every family (`fam:uri`, `base:uri`, `scope:@x`, `fork:busboy`), so the
//! npm package `uri` is NOT in the listed family `uri` (fast-uri, uri-js)
//! here, while the matcher's unprefixed spelling joins them (and `aws-sdk`
//! with `@aws-sdk/*`). The scorer's cards were written under libix's rule;
//! changing it would be a new scorer version.
//!
//! Two packages are one family when libix's `family` agrees, or when the
//! metadata-derived links (`families-derived.json`) connect them —
//! pairwise, never through a scope family.

use std::collections::HashMap;

use serde::Deserialize;

use humanify_ag_core::libindex::families::{DERIVED_JSON, FAMILIES_JSON};

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

#[derive(Deserialize)]
struct DerivedFile {
    links: Vec<DerivedLink>,
}

#[derive(Deserialize)]
struct DerivedLink {
    a: String,
    b: String,
}

/// libix's family table (`families.json`).
pub struct Libix {
    families: Vec<FamilyRow>,
    forks: HashMap<String, String>,
}

impl Libix {
    pub fn shipped() -> Libix {
        let file: FamiliesFile =
            serde_json::from_str(FAMILIES_JSON).expect("the shipped families.json parses");
        let mut forks = HashMap::new();
        for (a, b) in &file.rules.scoped_forks {
            forks.insert(a.clone(), format!("fork:{a}"));
            forks.insert(b.clone(), format!("fork:{a}"));
        }
        Libix {
            families: file.families,
            forks,
        }
    }

    /// `libix.family(p)`.
    pub fn family(&self, p: &str) -> String {
        for f in &self.families {
            for m in &f.members {
                let hit = match m.strip_suffix('*') {
                    Some(prefix) => p.starts_with(prefix),
                    None => m == p,
                };
                if hit {
                    return format!("fam:{}", f.name);
                }
            }
        }
        if let Some(fork) = self.forks.get(p) {
            return fork.clone();
        }
        if p.starts_with('@') {
            return format!("scope:{}", p.split('/').next().unwrap_or(p));
        }
        format!("base:{p}")
    }

    /// `family(a) == family(b)`.
    pub fn same(&self, a: &str, b: &str) -> bool {
        self.family(a) == self.family(b)
    }
}

/// `families_lib.Families`: libix's families joined by the derived links.
pub struct Families {
    pub libix: Libix,
    root: HashMap<String, String>,
}

impl Families {
    pub fn shipped() -> Families {
        let file: DerivedFile =
            serde_json::from_str(DERIVED_JSON).expect("the shipped families-derived.json parses");
        let mut parent: HashMap<String, String> = HashMap::new();
        fn find(p: &HashMap<String, String>, a: &str) -> String {
            let mut root = a.to_string();
            while let Some(next) = p.get(&root) {
                if *next == root {
                    break;
                }
                root = next.clone();
            }
            root
        }
        for l in &file.links {
            let (ra, rb) = (find(&parent, &l.a), find(&parent, &l.b));
            if ra != rb {
                let (lo, hi) = if ra < rb { (ra, rb) } else { (rb, ra) };
                parent.insert(hi, lo);
            }
        }
        let root = parent
            .keys()
            .map(|k| (k.clone(), find(&parent, k)))
            .collect();
        Families {
            libix: Libix::shipped(),
            root,
        }
    }

    fn find<'a>(&'a self, a: &'a str) -> &'a str {
        self.root.get(a).map_or(a, String::as_str)
    }

    /// `Families.same(a, b)`; None is never one family with anything.
    pub fn same(&self, a: Option<&str>, b: Option<&str>) -> bool {
        let (Some(a), Some(b)) = (a, b) else {
            return false;
        };
        self.libix.same(a, b) || self.find(a) == self.find(b)
    }
}

/// `families_lib.derived_sha()`: the derived links' canonical identity.
pub fn derived_sha() -> String {
    let v = crate::json::py::PyValue::parse(DERIVED_JSON).expect("derived links parse");
    crate::committed::sha256_hex(crate::json::py_text(&v, true).as_bytes())
}

#[cfg(test)]
#[path = "families_test.rs"]
mod families_test;
