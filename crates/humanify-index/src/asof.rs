//! As-of version selection (docs/design/library-index.md §3).
//!
//! `--as-of D` admits a version only when its registry publish time
//! (`time[version]` in the packument) is ≤ D. Among the admitted,
//! non-prerelease versions, the lab's policy (exp095 `scale/scripts/
//! pick.mjs`) picks `latest` (the highest — never `dist-tags.latest`,
//! which is TODAY's tag), `patch` (the highest earlier patch of that
//! major.minor) and `major-1` / `major-2` (the highest of each of the two
//! previous majors; for 0.x the minor plays the major). A pinned
//! `name@version` is indexed as `pinned` if it was published by D.
//!
//! Every time is compared in one normalised spelling,
//! `YYYY-MM-DDTHH:MM:SS.mmmZ`, so text order is time order.

use std::collections::{BTreeMap, BTreeSet};

use humanify_core::libindex::format::Semver;
use serde::{Deserialize, Serialize};

/// What the builder records of a package's registry document.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Packument {
    pub name: String,
    /// When the snapshot was taken (an input; recorded in the manifest).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fetched_at: Option<String>,
    /// version → publish time (also `created` / `modified`, ignored).
    pub time: BTreeMap<String, String>,
    /// The versions installable today.
    pub versions: Vec<String>,
}

impl Packument {
    pub fn from_json(text: &str) -> Result<Packument, String> {
        serde_json::from_str(text).map_err(|e| format!("packument: {e}"))
    }
}

/// One version to index.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selected {
    pub version: String,
    /// `latest`, `patch`, `major-1`, `major-2` or `pinned`.
    pub role: String,
    pub published: String,
}

/// A version the plan will not index, and why: `published-after-as-of`,
/// `unpublished` (in `time` but no longer installable), `not-in-registry`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refused {
    pub version: String,
    pub reason: String,
}

/// One package's plan.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PackagePlan {
    pub selected: Vec<Selected>,
    /// Only versions someone would have wanted: pins, and the versions
    /// today's policy would pick that the date (or the registry) refuses.
    pub refused: Vec<Refused>,
}

/// `YYYY-MM-DDTHH:MM:SS[.fff…]Z` → `YYYY-MM-DDTHH:MM:SS.mmmZ`; None for
/// anything else (only UTC is accepted: the registry writes UTC).
pub fn normalize_time(s: &str) -> Option<String> {
    let body = s.strip_suffix('Z')?;
    let (secs, frac) = match body.split_once('.') {
        Some((a, f)) => (a, f),
        None => (body, ""),
    };
    let b = secs.as_bytes();
    let shape = b.len() == 19
        && b.iter().enumerate().all(|(i, c)| match i {
            4 | 7 => *c == b'-',
            10 => *c == b'T',
            13 | 16 => *c == b':',
            _ => c.is_ascii_digit(),
        });
    if !shape || !frac.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let millis: String = frac.chars().chain("000".chars()).take(3).collect();
    Some(format!("{secs}.{millis}Z"))
}

/// `--as-of`: a date `YYYY-MM-DD` is the END of that UTC day; otherwise a
/// UTC timestamp.
pub fn parse_as_of(s: &str) -> Result<String, String> {
    if s.len() == 10
        && let Some(t) = normalize_time(&format!("{s}T23:59:59.999Z"))
    {
        return Ok(t);
    }
    normalize_time(s).ok_or_else(|| {
        format!("--as-of {s}: expected YYYY-MM-DD or a UTC time YYYY-MM-DDTHH:MM:SS[.mmm]Z")
    })
}

/// The lab's policy over stable versions (any order): (role, version).
fn policy(stable: &[(Semver, &str)]) -> Vec<(&'static str, String)> {
    let Some((latest, latest_v)) = stable.iter().max_by(|a, b| a.0.cmp(&b.0)) else {
        return Vec::new();
    };
    let line = |s: &Semver| {
        if s.major == 0 {
            (0, s.minor)
        } else {
            (s.major, 0)
        }
    };
    let mut out = vec![("latest", (*latest_v).to_string())];
    if let Some((_, v)) = stable
        .iter()
        .filter(|(s, _)| s.major == latest.major && s.minor == latest.minor && s < latest)
        .max_by(|a, b| a.0.cmp(&b.0))
    {
        out.push(("patch", (*v).to_string()));
    }
    let lines: BTreeSet<(u64, u64)> = stable
        .iter()
        .map(|(s, _)| line(s))
        .filter(|l| *l != line(latest))
        .collect();
    for (role, l) in ["major-1", "major-2"].into_iter().zip(lines.iter().rev()) {
        if let Some((_, v)) = stable
            .iter()
            .filter(|(s, _)| line(s) == *l)
            .max_by(|a, b| a.0.cmp(&b.0))
        {
            out.push((role, (*v).to_string()));
        }
    }
    out
}

/// Which versions of a package an index holds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum VersionPolicy {
    /// `latest`, `patch`, `major-1`, `major-2` (the scale lab's policy).
    #[default]
    AsOf,
    /// Every release of the 12 months before the as-of date (`recent`),
    /// plus the latest patch of every older minor (`minor`) — exp095
    /// density lab: 83.0% of every-release recall for 43% of the releases.
    YearAllMinor,
    /// Nothing but the pins (`--pinned`: the bundle's readers named them).
    PinsOnly,
}

impl VersionPolicy {
    pub fn from_name(name: &str) -> Option<VersionPolicy> {
        match name {
            "as-of" => Some(VersionPolicy::AsOf),
            "year-all+minor" => Some(VersionPolicy::YearAllMinor),
            _ => None,
        }
    }
}

/// `year-all+minor` over stable versions with their publish times:
/// (role, version), highest first.
fn year_all_minor(
    stable: &[(Semver, &str)],
    as_of: &str,
    published: &dyn Fn(&str) -> Option<String>,
) -> Vec<(&'static str, String)> {
    // twelve months before a normalised time: the same instant a year earlier
    let since = match as_of.get(..4).and_then(|y| y.parse::<u32>().ok()) {
        Some(y) => format!("{:04}{}", y - 1, &as_of[4..]),
        None => return Vec::new(),
    };
    let mut sorted: Vec<&(Semver, &str)> = stable.iter().collect();
    sorted.sort_by(|a, b| b.0.cmp(&a.0));
    let mut minors_seen: BTreeSet<(u64, u64)> = BTreeSet::new();
    let mut out = Vec::new();
    for (s, v) in sorted {
        let newest_of_minor = minors_seen.insert((s.major, s.minor));
        let recent =
            published(v).is_some_and(|t| t.as_str() > since.as_str() && t.as_str() <= as_of);
        if recent {
            out.push(("recent", (*v).to_string()));
        } else if newest_of_minor {
            out.push(("minor", (*v).to_string()));
        }
    }
    out
}

/// The versions of `p` to index as of `as_of` (normalised) under
/// `policy`, plus `pins`.
pub fn select_versions(
    p: &Packument,
    as_of: &str,
    pins: &[String],
    kind: VersionPolicy,
) -> PackagePlan {
    let installable: BTreeSet<&str> = p.versions.iter().map(String::as_str).collect();
    let published = |v: &str| p.time.get(v).and_then(|t| normalize_time(t));
    // Every stable, installable version with a publish time — today.
    let stable_today: Vec<(Semver, &str)> = p
        .versions
        .iter()
        .filter_map(|v| {
            let s = Semver::parse(v)?;
            (!s.is_prerelease() && published(v).is_some()).then_some((s, v.as_str()))
        })
        .collect();
    let admitted: Vec<(Semver, &str)> = stable_today
        .iter()
        .filter(|(_, v)| published(v).is_some_and(|t| t.as_str() <= as_of))
        .cloned()
        .collect();
    let pick = |versions: &[(Semver, &str)]| match kind {
        VersionPolicy::AsOf => policy(versions),
        VersionPolicy::YearAllMinor => year_all_minor(versions, as_of, &published),
        VersionPolicy::PinsOnly => Vec::new(),
    };
    let mut plan = PackagePlan::default();
    for (role, v) in pick(&admitted) {
        plan.selected.push(Selected {
            published: published(&v).unwrap_or_default(),
            version: v,
            role: role.to_string(),
        });
    }
    let refuse = |plan: &mut PackagePlan, v: &str, reason: &str| {
        if !plan.refused.iter().any(|r| r.version == v) {
            plan.refused.push(Refused {
                version: v.to_string(),
                reason: reason.to_string(),
            });
        }
    };
    // What today's policy would pick that the date refuses.
    for (_, v) in pick(&stable_today) {
        if !plan.selected.iter().any(|s| s.version == v) {
            refuse(&mut plan, &v, "published-after-as-of");
        }
    }
    // A version the registry still dates but no longer serves.
    for v in p.time.keys() {
        if v != "created" && v != "modified" && !installable.contains(v.as_str()) {
            refuse(&mut plan, v, "unpublished");
        }
    }
    for pin in pins {
        if plan.selected.iter().any(|s| &s.version == pin) {
            continue;
        }
        match published(pin) {
            _ if !installable.contains(pin.as_str()) => refuse(&mut plan, pin, "not-in-registry"),
            Some(t) if t.as_str() <= as_of => plan.selected.push(Selected {
                version: pin.clone(),
                role: "pinned".to_string(),
                published: t,
            }),
            _ => refuse(&mut plan, pin, "published-after-as-of"),
        }
    }
    plan
}
