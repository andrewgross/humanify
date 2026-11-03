//! The index spec's parse and validation: one committed TOML file that
//! says how an index is made. No registry access here — parse only.

use super::asof::VersionPolicy;
use super::build::SpongeRegions;
use super::spec::{
    HighImpactList, IndexSpec, Source, VersionRules, name_matches, parse_high_impact_list,
};

const VALID: &str = r#"
schema = "humanify-index-spec/1"
as_of = "2026-10-25"

[build]
recipes = ["bun:target=bun,format=cjs,minify,production"]
sponge_regions = "origin"
names = true

[tools]
bun = "1.4.0"

[[select]]
id = "impact"
source = "npm-high-impact"
list = "top"
version = "1.13.0"
sha256 = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"

[[select]]
id = "dep-trees"
source = "dependencies"
of = ["impact"]

[versions]
policy = "year-all+major"
dependents = true

[[versions.override]]
id = "aws-sdks"
policy = "latest-200"
names = ["@aws-sdk/*"]

[[refuse]]
id = "types"
reason = "type declarations, no JavaScript to fingerprint"
names = ["@types/*"]
"#;

#[test]
fn a_spec_parses_into_selections_rules_and_tools() {
    let spec = IndexSpec::parse(VALID).expect("spec");
    assert_eq!(spec.as_of, "2026-10-25T23:59:59.999Z");
    assert_eq!(spec.sponge_regions, SpongeRegions::Origin);
    assert!(spec.names);
    assert_eq!(spec.select.len(), 2);
    assert_eq!(spec.select[0].id, "impact");
    the_list_selection_is_read(&spec.select[0].source);
    assert_eq!(
        spec.select[1].source,
        Source::Dependencies {
            of: vec!["impact".into()]
        }
    );
    the_version_rules_are_read(&spec.versions);
    assert_eq!(spec.refuse.len(), 1);
    assert_eq!(
        spec.refuse[0].reason,
        "type declarations, no JavaScript to fingerprint"
    );
}

/// The REAL `npm-high-impact@1.13.0` `lib/top.js` shape: the last entry
/// has NO trailing comma and the array closes with a bare `]` — the
/// parser failed on exactly this before exp184's fix ("expected a quoted
/// name, got \"'babel-plugin-espower'\"").
#[test]
fn the_real_list_file_shape_parses() {
    let text = "export const top = [\n  'semver',\n  'minimatch',\n  'debug',\n  'babel-plugin-espower'\n]\n";
    let (names, rejects) = parse_high_impact_list(text, HighImpactList::Top).expect("parses");
    assert_eq!(
        names,
        ["semver", "minimatch", "debug", "babel-plugin-espower"]
    );
    assert!(rejects.is_empty());
    // the comma-carrying shape parses too (a defensive check, not the bug)
    let commas = "export const top = [\n  'semver',\n  'debug',\n];\n";
    let (names, rejects) = parse_high_impact_list(commas, HighImpactList::Top).expect("parses");
    assert_eq!(names, ["semver", "debug"]);
    assert!(rejects.is_empty());
}

/// The real list carries a bare `-` entry (npm-high-impact 1.13.0, line
/// 17256) — a sentinel, not a package: `npm view -` 404s. An entry that
/// is not a plausible npm name (every `/`-part contains a letter or a
/// digit) is NOT a parse error — the pinned list carries the sentinel
/// permanently — it comes back as a REJECT with its line, for the
/// resolver to record as a skip.
#[test]
fn a_list_entry_that_is_not_a_package_name_is_a_reject() {
    let text = "export const top = [\n  'semver',\n  '-',\n  'debug',\n]\n";
    let (names, rejects) = parse_high_impact_list(text, HighImpactList::Top).expect("parses");
    assert_eq!(names, ["semver", "debug"]);
    assert_eq!(rejects, ["-"]);
    // scoped names still parse
    let scoped = "export const top = [\n  '@babel/traverse',\n]\n";
    let (names, rejects) = parse_high_impact_list(scoped, HighImpactList::Top).expect("parses");
    assert_eq!(names, ["@babel/traverse"]);
    assert!(rejects.is_empty());
}

fn the_list_selection_is_read(source: &Source) {
    match source {
        Source::NpmHighImpact {
            list,
            version,
            sha256,
            limit,
        } => {
            assert_eq!(*list, HighImpactList::Top);
            assert_eq!(version, "1.13.0");
            assert_eq!(sha256.as_str(), "a".repeat(64));
            assert_eq!(*limit, None);
        }
        other => panic!("{other:?}"),
    }
}

fn the_version_rules_are_read(rules: &VersionRules) {
    assert_eq!(rules.default, VersionPolicy::YearAllMajor);
    assert!(rules.dependents);
    assert_eq!(rules.overrides.len(), 1);
    assert_eq!(rules.overrides[0].policy, VersionPolicy::Latest(200));
    assert_eq!(rules.overrides[0].when.names.as_slice(), ["@aws-sdk/*"]);
}

#[test]
fn the_schema_is_checked_and_the_fields_are_closed() {
    let bad = VALID.replace("schema = \"humanify-index-spec/1\"", "schema = \"other/1\"");
    assert!(IndexSpec::parse(&bad).is_err());
    let bad = format!("{VALID}\n[build]\nsurprise = 1");
    assert!(IndexSpec::parse(&bad).is_err(), "unknown fields refused");
    let bad = VALID.replace("sponge_regions = \"origin\"", "sponge_regions = \"keep\"");
    assert!(IndexSpec::parse(&bad).is_err());
}

#[test]
fn ids_must_be_simple_and_unique() {
    let bad = VALID.replace("id = \"dep-trees\"", "id = \"impact\"");
    assert!(
        IndexSpec::parse(&bad)
            .err()
            .is_some_and(|e| e.contains("given twice")),
        "two selections with one id"
    );
    let bad = VALID.replace("id = \"impact\"", "id = \"impact top!\"");
    assert!(
        IndexSpec::parse(&bad)
            .err()
            .is_some_and(|e| e.contains("letters, digits")),
        "id characters"
    );
}

#[test]
fn a_dependencies_selection_may_only_name_earlier_selections() {
    let bad = VALID.replace(
        "[[select]]\nid = \"dep-trees\"\nsource = \"dependencies\"\nof = [\"impact\"]",
        "[[select]]\nid = \"dep-trees\"\nsource = \"dependencies\"\nof = [\"later\"]",
    );
    assert!(
        IndexSpec::parse(&bad)
            .err()
            .is_some_and(|e| e.contains("not an earlier selection")),
        "a forward reference"
    );
    // and the same rule for an override's `selection`
    let bad = VALID.replace(
        "[[versions.override]]\nid = \"aws-sdks\"\npolicy = \"latest-200\"\nnames = [\"@aws-sdk/*\"]",
        "[[versions.override]]\nid = \"aws-sdks\"\npolicy = \"latest-200\"\nselection = \"none-such\"\nrank_below = 100",
    );
    assert!(
        IndexSpec::parse(&bad)
            .err()
            .is_some_and(|e| e.contains("not a selection id")),
        "an override's selection must exist"
    );
}

#[test]
fn a_list_selection_needs_its_version_and_hash() {
    let bad = VALID.replace(
        "version = \"1.13.0\"\nsha256 = \"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"\n",
        "",
    );
    assert!(
        IndexSpec::parse(&bad)
            .err()
            .is_some_and(|e| e.contains("`version`")),
        "no version"
    );
    let bad = VALID.replace(
        "sha256 = \"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"",
        "sha256 = \"short\"",
    );
    assert!(
        IndexSpec::parse(&bad)
            .err()
            .is_some_and(|e| e.contains("64 hex")),
        "a short hash"
    );
    // a dependencies selection takes no list fields
    let bad = VALID.replace(
        "of = [\"impact\"]",
        "of = [\"impact\"]\nversion = \"1.13.0\"",
    );
    assert!(
        IndexSpec::parse(&bad)
            .err()
            .is_some_and(|e| e.contains("takes only `of`")),
        "a list field on a dependencies selection"
    );
}

#[test]
fn tools_must_cover_the_recipes_and_nothing_else() {
    let bad = VALID.replace("[tools]\nbun = \"1.4.0\"", "[tools]\nesbuild = \"0.28.2\"");
    assert!(
        IndexSpec::parse(&bad)
            .err()
            .is_some_and(|e| e.contains("needs tools.bun")),
        "a recipe without its tool"
    );
    let bad = VALID.replace(
        "[tools]\nbun = \"1.4.0\"",
        "[tools]\nbun = \"1.4.0\"\nrollup = \"4.64.5\"",
    );
    assert!(
        IndexSpec::parse(&bad)
            .err()
            .is_some_and(|e| e.contains("no recipe uses")),
        "an unused tool"
    );
    let bad = VALID.replace("bun = \"1.4.0\"", "bun = \"not-a-version\"");
    assert!(
        IndexSpec::parse(&bad)
            .err()
            .is_some_and(|e| e.contains("not a version")),
        "a tool version that is not one"
    );
    let bad = VALID.replace("bun = \"1.4.0\"", "java = \"1.4.0\"");
    assert!(
        IndexSpec::parse(&bad)
            .err()
            .is_some_and(|e| e.contains("not a recipe bundler")),
        "an unknown bundler"
    );
}

#[test]
fn overrides_and_refusals_must_match_something() {
    // an override matching every package is the default policy's job
    let bad = VALID.replace(
        "[[versions.override]]\nid = \"aws-sdks\"\npolicy = \"latest-200\"\nnames = [\"@aws-sdk/*\"]",
        "[[versions.override]]\nid = \"aws-sdks\"\npolicy = \"latest-200\"",
    );
    assert!(
        IndexSpec::parse(&bad)
            .err()
            .is_some_and(|e| e.contains("matches every package")),
        "an override with no matcher"
    );
    let bad = VALID.replace(
        "id = \"aws-sdks\"\npolicy = \"latest-200\"\nnames = [\"@aws-sdk/*\"]",
        "id = \"aws-sdks\"\npolicy = \"latest-200\"\nnames = [\"@aws-sdk/*\"]\nrank_below = 100",
    );
    assert!(
        IndexSpec::parse(&bad)
            .err()
            .is_some_and(|e| e.contains("rank_below needs `selection`")),
        "a rank without a selection"
    );
    let bad = VALID.replace(
        "[[refuse]]\nid = \"types\"\nreason = \"type declarations, no JavaScript to fingerprint\"\nnames = [\"@types/*\"]",
        "[[refuse]]\nid = \"types\"\nreason = \"type declarations, no JavaScript to fingerprint\"",
    );
    assert!(
        IndexSpec::parse(&bad)
            .err()
            .is_some_and(|e| e.contains("exactly one")),
        "a refusal with no matcher"
    );
    let bad = VALID.replace(
        "reason = \"type declarations, no JavaScript to fingerprint\"",
        "reason = \"\"",
    );
    assert!(
        IndexSpec::parse(&bad)
            .err()
            .is_some_and(|e| e.contains("needs a reason")),
        "an empty reason"
    );
    let bad = VALID.replace("names = [\"@types/*\"]", "versions = [\"left-pad\"]");
    assert!(
        IndexSpec::parse(&bad)
            .err()
            .is_some_and(|e| e.contains("not name@version")),
        "a refusal version without a version"
    );
    let bad = VALID.replace("names = [\"@types/*\"]", "platform = false");
    assert!(
        IndexSpec::parse(&bad)
            .err()
            .is_some_and(|e| e.contains("matches nothing")),
        "platform = false"
    );
}

#[test]
fn name_patterns_are_exact_or_a_prefix_star() {
    assert!(name_matches("ms", "ms"));
    assert!(!name_matches("ms", "msx"));
    assert!(name_matches("@aws-sdk/*", "@aws-sdk/client-s3"));
    assert!(!name_matches("@aws-sdk/*", "@aws-sdk"));
    assert!(name_matches("is-*", "is-odd"));
    assert!(!name_matches("is-*", "is"));
    assert!(
        !name_matches("*", "anything"),
        "a bare star is not a pattern"
    );
}

#[test]
fn refusals_take_several_kinds_of_matcher() {
    let spec = IndexSpec::parse(&format!(
        "{VALID}\n[[refuse]]\nid = \"big\"\nreason = \"too big to fingerprint\"\nunpacked_size_over = 52428800\n[[refuse]]\nid = \"plat\"\nreason = \"a platform binary\"\nplatform = true\n[[refuse]]\nid = \"exact\"\nreason = \"known junk\"\nversions = [\"ms@0.1.0\"]"
    ))
    .expect("spec");
    assert_eq!(spec.refuse.len(), 4);
    assert!(matches!(
        spec.refuse[1].when,
        super::spec::RefusalMatch::UnpackedSizeOver(52_428_800)
    ));
    assert!(matches!(
        spec.refuse[2].when,
        super::spec::RefusalMatch::Platform
    ));
    assert_eq!(
        spec.refuse[3].when,
        super::spec::RefusalMatch::Versions(vec!["ms@0.1.0".into()])
    );
}

#[test]
fn a_list_limit_is_kept() {
    let spec = IndexSpec::parse(&VALID.replace(
        "sha256 = \"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"",
        "sha256 = \"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"\nlimit = 2000",
    ))
    .expect("spec");
    match &spec.select[0].source {
        Source::NpmHighImpact { limit, .. } => assert_eq!(*limit, Some(2000)),
        other => panic!("{other:?}"),
    }
}
