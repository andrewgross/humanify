//! Package families (docs/design/library-index.md §7): a claim naming
//! another member of the true package's family is family-correct, not
//! wrong. ONE reader of `families.json`, the data the index ships
//! (`humanify_index::merge` copies [`FAMILIES_JSON`] into every index) and
//! the lookup side reads — library discovery's isolation veto asks it which
//! neighbours are "the same package" (docs/design/library-discovery.md §3b).

use serde::Deserialize;

/// The shipped families.
pub const FAMILIES_JSON: &str = include_str!("families.json");

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
}

impl PackageFamilies {
    /// The shipped families.
    pub fn shipped() -> PackageFamilies {
        let file: FamiliesFile =
            serde_json::from_str(FAMILIES_JSON).expect("the shipped families.json parses");
        PackageFamilies {
            families: file.families,
            scoped_forks: file.rules.scoped_forks,
        }
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
