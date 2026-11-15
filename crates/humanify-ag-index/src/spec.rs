//! The index spec (exp184, Andrew 2026-11-02): ONE committed, declarative
//! file that says how an index is made — the as-of date, which packages
//! (rules over public, replayable signals), which versions of each (rules,
//! with per-package overrides that are rules too), the recipes and the
//! pinned tools, and the refusals (rules with reasons). `humanify-ag
//! index build --spec <file>` turns it into an index with no other input than the
//! public npm registry; the resolved plan (every package@version and the
//! rule that chose it) is recorded in the index's `build.json`.
//!
//! This module only READS a spec: it parses and validates the file into
//! [`IndexSpec`]. Resolving it against the registry is
//! [`crate::planner`].

use std::collections::BTreeMap;

use humanify_ag_core::libindex::recipe::{Recipe, RecipeBundler};
use serde::Deserialize;

use crate::asof::{VersionPolicy, parse_as_of};
use crate::build::SpongeRegions;

/// The spec format this builder reads.
pub const SPEC_SCHEMA: &str = "humanify-index-spec/1";

/// A parsed, validated spec.
#[derive(Clone, Debug, PartialEq)]
pub struct IndexSpec {
    /// The normalised as-of time (`YYYY-MM-DD` = the end of that UTC day).
    pub as_of: String,
    pub recipes: Vec<Recipe>,
    pub sponge_regions: SpongeRegions,
    pub names: bool,
    /// bundler → its pinned version (every recipe's bundler has one).
    pub tools: BTreeMap<RecipeBundler, String>,
    /// Package selections, in spec order.
    pub select: Vec<Selection>,
    pub versions: VersionRules,
    pub refuse: Vec<Refusal>,
}

/// Where a selection's packages come from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    /// A list exported by the `npm-high-impact` package (npm's own
    /// "high-impact" definition: ≥ 1M downloads a week or ≥ 500
    /// dependents), read from the package at a pinned version published
    /// on or before the as-of date — an immutable npm tarball, so the
    /// list replays exactly. `sha256` is the list file's, verified.
    NpmHighImpact {
        list: HighImpactList,
        version: String,
        sha256: String,
        limit: Option<usize>,
    },
    /// Every package the install trees of the `of` selections' chosen
    /// versions hold: each version's dependencies (and optional and
    /// non-optional peer dependencies) resolved as `npm install
    /// --before=<its publish time>` resolves them, transitively.
    Dependencies { of: Vec<String> },
}

/// Which list of `npm-high-impact`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HighImpactList {
    /// `npmHighImpact` (`lib/top.js`): the union, most downloaded first.
    Top,
    /// `npmTopDownloads` (`lib/top-download.js`).
    TopDownload,
    /// `npmTopDependents` (`lib/top-dependent.js`).
    TopDependent,
}

impl HighImpactList {
    pub fn file(self) -> &'static str {
        match self {
            HighImpactList::Top => "lib/top.js",
            HighImpactList::TopDownload => "lib/top-download.js",
            HighImpactList::TopDependent => "lib/top-dependent.js",
        }
    }

    /// The `export const` the list file declares.
    pub fn const_name(self) -> &'static str {
        match self {
            HighImpactList::Top => "top",
            HighImpactList::TopDownload => "topDownload",
            HighImpactList::TopDependent => "topDependent",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selection {
    pub id: String,
    pub source: Source,
}

/// Which versions each package is indexed at: the first override whose
/// match holds, else the default policy; then the `dependents` rule adds
/// what ranges resolve to.
/// A spec's version strategy: one policy, or several whose choices are
/// UNIONED (`policy = ["as-of", "recent-18mo"]`: the older major lines
/// as-of keeps, plus every release of the window).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Strategy(pub Vec<VersionPolicy>);

impl Strategy {
    /// As the spec spells it, comma-joined (the plan's attribution).
    pub fn name(&self) -> String {
        self.0
            .iter()
            .map(VersionPolicy::name)
            .collect::<Vec<_>>()
            .join(",")
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct VersionRules {
    pub default: Strategy,
    pub overrides: Vec<VersionOverride>,
    /// Add, for every package in the plan, up to this many of the
    /// versions its dependents' ranges resolve to (each range as of the
    /// dependent's own publish time: what an install that day got —
    /// computable from packuments, unlike per-version download counts),
    /// the most-resolved first. 0 turns the rule off.
    pub dependents: usize,
    /// An explicit depth over the default policy: at most this many of
    /// the versions it chose, the newest first (None: uncapped).
    pub max: Option<usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VersionOverride {
    pub id: String,
    pub when: PackageMatch,
    pub policy: Strategy,
    /// The override's own depth cap (see [`VersionRules::max`]).
    pub max: Option<usize>,
}

/// Which packages a rule applies to. Every field given must hold.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PackageMatch {
    /// Name patterns: `@scope/*`, `prefix-*`, or an exact name.
    pub names: Vec<String>,
    /// Chosen by this selection...
    pub selection: Option<String>,
    /// ...within its first N (a list source's rank).
    pub rank_below: Option<usize>,
}

/// A refusal rule: matched versions leave the plan, recorded with the
/// rule's id and reason.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refusal {
    pub id: String,
    pub reason: String,
    pub when: RefusalMatch,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RefusalMatch {
    /// Every version of a package whose name matches.
    Names(Vec<String>),
    /// A version whose `package.json` restricts `os` or `cpu` (a
    /// platform binary, no JavaScript to fingerprint).
    Platform,
    /// A version whose unpacked tarball is larger than this many bytes.
    UnpackedSizeOver(u64),
    /// Exact `name@version`s — for a case no rule above expresses.
    Versions(Vec<String>),
}

// ---- the file's shape ----

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSpec {
    schema: String,
    as_of: String,
    build: RawBuild,
    tools: BTreeMap<String, String>,
    select: Vec<RawSelection>,
    versions: RawVersions,
    #[serde(default)]
    refuse: Vec<RawRefusal>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawBuild {
    recipes: Vec<String>,
    sponge_regions: String,
    names: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSelection {
    id: String,
    source: String,
    list: Option<String>,
    version: Option<String>,
    sha256: Option<String>,
    limit: Option<usize>,
    of: Option<Vec<String>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawVersions {
    policy: RawStrategy,
    #[serde(default)]
    dependents: usize,
    max: Option<usize>,
    #[serde(default, rename = "override")]
    overrides: Vec<RawOverride>,
}

/// `policy = "as-of"` or `policy = ["as-of", "recent-18mo"]`.
#[derive(Deserialize)]
#[serde(untagged)]
enum RawStrategy {
    One(String),
    Many(Vec<String>),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawOverride {
    id: String,
    policy: RawStrategy,
    #[serde(default)]
    names: Vec<String>,
    selection: Option<String>,
    rank_below: Option<usize>,
    max: Option<usize>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRefusal {
    id: String,
    reason: String,
    names: Option<Vec<String>>,
    platform: Option<bool>,
    unpacked_size_over: Option<u64>,
    versions: Option<Vec<String>>,
}

/// Why an index spec is refused.
#[derive(Debug, thiserror::Error)]
pub enum SpecError {
    #[error("spec: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("spec: schema {0:?} (this builder reads {SPEC_SCHEMA:?})")]
    Schema(String),
    #[error("spec: {0}")]
    AsOf(#[from] crate::asof::AsOfError),
    #[error("spec: build.recipes: {0}")]
    Recipe(humanify_ag_core::libindex::recipe::RecipeError),
    #[error("spec: build.recipes: {0} given twice")]
    RecipeTwice(String),
    #[error("spec: build.sponge_regions {0:?} (drop, origin)")]
    SpongeRegions(String),
    #[error("spec: tools.{0}: not a recipe bundler")]
    NotABundler(String),
    #[error("spec: tools.{name} = {version:?}: not a version")]
    ToolVersion { name: String, version: String },
    #[error("spec: recipe {recipe} needs tools.{bundler} (its pinned version)")]
    NoTool {
        recipe: String,
        bundler: &'static str,
    },
    #[error("spec: tools.{0} pins a bundler no recipe uses")]
    UnusedTool(&'static str),
    /// A rule that is wrong in itself: where, and what is wrong.
    #[error("{ctx}: {why}")]
    Invalid { ctx: String, why: &'static str },
    #[error(
        "{ctx}: unknown version policy {text:?} (as-of, latest[-<N>], \
         recent-<N>mo, year-all+minor, year-all+major, \
         year-last<N>+major[<M>])"
    )]
    UnknownPolicy { ctx: String, text: String },
    #[error("{ctx}: policy {text:?} is listed twice")]
    PolicyTwice { ctx: String, text: String },
    #[error("{what} id {id:?}: use letters, digits, '-' and '_'")]
    BadId { what: &'static str, id: String },
    #[error("{what} id {id:?} given twice")]
    IdTwice { what: &'static str, id: String },
    #[error(
        "{ctx}: name pattern {pattern:?} (an exact name, or a prefix ending in one '*'; \
         a bare '*' is not a pattern)"
    )]
    NamePattern { ctx: String, pattern: String },
    #[error("{file}: expected a quoted name, got {line:?}")]
    ListLine { file: &'static str, line: String },
    #[error("{0}: the list never closes")]
    ListOpen(&'static str),
    #[error("{ctx}: list {list:?} (top, top-download, top-dependent)")]
    List { ctx: String, list: Option<String> },
    #[error("{ctx}: sha256 {sha256:?} is not 64 hex digits")]
    Sha256 { ctx: String, sha256: String },
    #[error("{ctx}: `of` names {of:?}, not an earlier selection")]
    NotEarlier { ctx: String, of: String },
    #[error("{ctx}: source {given:?} (npm-high-impact, dependencies)")]
    Source { ctx: String, given: String },
    #[error("{ctx}: {spec:?} is not name@version")]
    NotNameAtVersion { ctx: String, spec: String },
    #[error("{ctx}: selection {selection:?} is not a selection id")]
    NotASelection { ctx: String, selection: String },
}

fn invalid(ctx: &str, why: &'static str) -> SpecError {
    SpecError::Invalid {
        ctx: ctx.to_string(),
        why,
    }
}

fn policy(raw: &RawStrategy, ctx: &str) -> Result<Strategy, SpecError> {
    let names: Vec<&str> = match raw {
        RawStrategy::One(n) => vec![n.as_str()],
        RawStrategy::Many(ns) => ns.iter().map(String::as_str).collect(),
    };
    if names.is_empty() {
        return Err(invalid(ctx, "a policy list needs at least one strategy"));
    }
    let mut out: Vec<VersionPolicy> = Vec::new();
    for text in names {
        let p = VersionPolicy::from_name(text).ok_or_else(|| SpecError::UnknownPolicy {
            ctx: ctx.to_string(),
            text: text.to_string(),
        })?;
        if out.contains(&p) {
            return Err(SpecError::PolicyTwice {
                ctx: ctx.to_string(),
                text: text.to_string(),
            });
        }
        out.push(p);
    }
    Ok(Strategy(out))
}

fn check_id(id: &str, seen: &mut Vec<String>, what: &'static str) -> Result<(), SpecError> {
    if id.is_empty()
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(SpecError::BadId {
            what,
            id: id.to_string(),
        });
    }
    if seen.iter().any(|s| s == id) {
        return Err(SpecError::IdTwice {
            what,
            id: id.to_string(),
        });
    }
    seen.push(id.to_string());
    Ok(())
}

/// A depth cap of 0 would plan nothing: refused.
fn check_max(max: Option<usize>, ctx: &str) -> Result<(), SpecError> {
    match max {
        Some(0) => Err(invalid(ctx, "max must be at least 1 (omit it for no cap)")),
        _ => Ok(()),
    }
}

fn check_names(names: &[String], ctx: &str) -> Result<(), SpecError> {
    for n in names {
        let stem = n.strip_suffix('*').unwrap_or(n);
        if n.is_empty() || stem.is_empty() || stem.contains('*') {
            return Err(SpecError::NamePattern {
                ctx: ctx.to_string(),
                pattern: n.clone(),
            });
        }
    }
    Ok(())
}

/// Whether `name` matches a pattern: exact, or a prefix ending in `*`
/// (a bare `*` matches nothing — a rule that matches every package is
/// the default policy's job, not a pattern's).
pub fn name_matches(pattern: &str, name: &str) -> bool {
    match pattern.strip_suffix('*') {
        Some(prefix) if !prefix.is_empty() => name.starts_with(prefix),
        Some(_) => false,
        None => pattern == name,
    }
}

/// The package names a list file declares, in order, plus the entries
/// that are NOT package names (the real list carries a bare `-` sentinel
/// entry) with their texts, for the resolver to record as skips. The
/// files are `export const top = [\n  'name',\n …\n];` — one
/// single-quoted name per line, the last without its comma, the array
/// closed by `];` (or `]`).
pub fn parse_high_impact_list(
    text: &str,
    list: HighImpactList,
) -> Result<(Vec<String>, Vec<String>), SpecError> {
    let opener = format!("export const {} = [", list.const_name());
    let mut names = Vec::new();
    let mut rejects = Vec::new();
    let mut inside = false;
    for line in text.lines() {
        let t = line.trim();
        if !inside {
            if t == opener {
                inside = true;
            }
            continue;
        }
        let bare = t.strip_suffix(';').unwrap_or(t);
        if bare == "]" {
            return Ok((names, rejects));
        }
        let name = match t
            .strip_prefix('\'')
            .and_then(|r| r.strip_suffix("',").or_else(|| r.strip_suffix('\'')))
        {
            Some(n) => n,
            None => {
                return Err(SpecError::ListLine {
                    file: list.file(),
                    line: t.to_string(),
                });
            }
        };
        if name.is_empty()
            || !name
                .split('/')
                .all(|p| p.bytes().any(|b| b.is_ascii_alphanumeric()))
        {
            rejects.push(name.to_string());
            continue;
        }
        names.push(name.to_string());
    }
    Err(SpecError::ListOpen(list.file()))
}

fn selection(r: RawSelection, earlier: &[Selection]) -> Result<Selection, SpecError> {
    let ctx = format!("select {:?}", r.id);
    let source = match r.source.as_str() {
        "npm-high-impact" => {
            if r.of.is_some() {
                return Err(invalid(&ctx, "`of` is for source = \"dependencies\""));
            }
            let list = match r.list.as_deref() {
                Some("top") => HighImpactList::Top,
                Some("top-download") => HighImpactList::TopDownload,
                Some("top-dependent") => HighImpactList::TopDependent,
                other => {
                    return Err(SpecError::List {
                        ctx,
                        list: other.map(str::to_string),
                    });
                }
            };
            let version = r
                .version
                .ok_or_else(|| invalid(&ctx, "needs the list package's `version`"))?;
            let sha256 = r
                .sha256
                .ok_or_else(|| invalid(&ctx, "needs the list file's `sha256`"))?;
            if sha256.len() != 64 || !sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err(SpecError::Sha256 { ctx, sha256 });
            }
            Source::NpmHighImpact {
                list,
                version,
                sha256: sha256.to_ascii_lowercase(),
                limit: r.limit,
            }
        }
        "dependencies" => {
            if r.list.is_some() || r.version.is_some() || r.sha256.is_some() || r.limit.is_some() {
                return Err(invalid(&ctx, "a dependencies selection takes only `of`"));
            }
            let of =
                r.of.ok_or_else(|| invalid(&ctx, "needs `of` (earlier selection ids)"))?;
            for o in &of {
                if !earlier.iter().any(|s| &s.id == o) {
                    return Err(SpecError::NotEarlier { ctx, of: o.clone() });
                }
            }
            Source::Dependencies { of }
        }
        other => {
            return Err(SpecError::Source {
                ctx,
                given: other.to_string(),
            });
        }
    };
    Ok(Selection { id: r.id, source })
}

fn refusal(r: RawRefusal) -> Result<Refusal, SpecError> {
    let ctx = format!("refuse {:?}", r.id);
    if r.reason.trim().is_empty() {
        return Err(invalid(&ctx, "needs a reason"));
    }
    let mut whens = Vec::new();
    if let Some(n) = r.names {
        check_names(&n, &ctx)?;
        whens.push(RefusalMatch::Names(n));
    }
    if let Some(p) = r.platform {
        if !p {
            return Err(invalid(
                &ctx,
                "platform = false matches nothing; drop the rule",
            ));
        }
        whens.push(RefusalMatch::Platform);
    }
    if let Some(s) = r.unpacked_size_over {
        whens.push(RefusalMatch::UnpackedSizeOver(s));
    }
    if let Some(v) = r.versions {
        for spec in &v {
            let (name, version) = crate::build::parse_package_spec(spec);
            if version.is_none() || name.is_empty() {
                return Err(SpecError::NotNameAtVersion {
                    ctx,
                    spec: spec.clone(),
                });
            }
        }
        whens.push(RefusalMatch::Versions(v));
    }
    let when = match whens.len() {
        1 => whens.pop().expect("one"),
        _ => {
            return Err(invalid(
                &ctx,
                "give exactly one of names, platform, unpacked_size_over, versions",
            ));
        }
    };
    Ok(Refusal {
        id: r.id,
        reason: r.reason,
        when,
    })
}

impl IndexSpec {
    /// Parse and validate a spec file's text.
    pub fn parse(text: &str) -> Result<IndexSpec, SpecError> {
        let raw: RawSpec = toml::from_str(text)?;
        if raw.schema != SPEC_SCHEMA {
            return Err(SpecError::Schema(raw.schema));
        }
        let as_of = parse_as_of(&raw.as_of)?;
        let mut recipes = Vec::new();
        for text in &raw.build.recipes {
            let r = Recipe::parse(text).map_err(SpecError::Recipe)?;
            if recipes.contains(&r) {
                return Err(SpecError::RecipeTwice(text.clone()));
            }
            recipes.push(r);
        }
        if recipes.is_empty() {
            return Err(invalid("spec", "build.recipes is empty"));
        }
        let sponge_regions = SpongeRegions::parse(&raw.build.sponge_regions)
            .ok_or_else(|| SpecError::SpongeRegions(raw.build.sponge_regions.clone()))?;
        let mut tools = BTreeMap::new();
        for (name, version) in &raw.tools {
            let bundler =
                RecipeBundler::parse(name).ok_or_else(|| SpecError::NotABundler(name.clone()))?;
            if humanify_ag_core::libindex::format::Semver::parse(version).is_none() {
                return Err(SpecError::ToolVersion {
                    name: name.clone(),
                    version: version.clone(),
                });
            }
            tools.insert(bundler, version.clone());
        }
        for r in &recipes {
            if !tools.contains_key(&r.bundler) {
                return Err(SpecError::NoTool {
                    recipe: r.spelling(),
                    bundler: r.bundler.name(),
                });
            }
        }
        if let Some(unused) = tools
            .keys()
            .find(|b| !recipes.iter().any(|r| r.bundler == **b))
        {
            return Err(SpecError::UnusedTool(unused.name()));
        }
        let mut ids = Vec::new();
        let mut select = Vec::new();
        for r in raw.select {
            check_id(&r.id, &mut ids, "select")?;
            let s = selection(r, &select)?;
            select.push(s);
        }
        if !select
            .iter()
            .any(|s| matches!(s.source, Source::NpmHighImpact { .. }))
        {
            return Err(invalid("spec", "no selection has a package source"));
        }
        let default = policy(&raw.versions.policy, "spec: versions.policy")?;
        check_max(raw.versions.max, "spec: versions")?;
        let mut overrides = Vec::new();
        for o in raw.versions.overrides {
            let ctx = format!("spec: versions.override {:?}", o.id);
            check_id(&o.id, &mut ids, "versions.override")?;
            check_names(&o.names, &ctx)?;
            if let Some(s) = &o.selection
                && !select.iter().any(|x| &x.id == s)
            {
                return Err(SpecError::NotASelection {
                    ctx,
                    selection: s.clone(),
                });
            }
            if o.rank_below.is_some() && o.selection.is_none() {
                return Err(invalid(&ctx, "rank_below needs `selection` (whose rank)"));
            }
            if o.names.is_empty() && o.selection.is_none() {
                return Err(invalid(
                    &ctx,
                    "matches every package; set versions.policy instead",
                ));
            }
            check_max(o.max, &ctx)?;
            overrides.push(VersionOverride {
                policy: policy(&o.policy, &ctx)?,
                max: o.max,
                id: o.id,
                when: PackageMatch {
                    names: o.names,
                    selection: o.selection,
                    rank_below: o.rank_below,
                },
            });
        }
        let mut refuse = Vec::new();
        for r in raw.refuse {
            check_id(&r.id, &mut ids, "refuse")?;
            refuse.push(refusal(r)?);
        }
        Ok(IndexSpec {
            as_of,
            recipes,
            sponge_regions,
            names: raw.build.names,
            tools,
            select,
            versions: VersionRules {
                default,
                overrides,
                dependents: raw.versions.dependents,
                max: raw.versions.max,
            },
            refuse,
        })
    }
}
