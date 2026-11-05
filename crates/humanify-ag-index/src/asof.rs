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

use humanify_ag_core::libindex::format::Semver;
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
    /// version → the per-version metadata the spec's dependency rules
    /// read (fetched with the time map since exp184). Absent in a
    /// snapshot taken before it: [`Packument::has_deps`] is the check,
    /// and a spec that needs the data fails loud on one without it.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub deps: BTreeMap<String, VersionMeta>,
    /// The snapshot carries per-version metadata ([`Packument::deps`],
    /// possibly all empty — no version of this package has any).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub deps_fetched: bool,
    /// The registry does not know this package (a 404, or an
    /// unpublished tombstone): recorded in the snapshot so an offline
    /// replay skips it exactly as the live fetch did.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub absent: bool,
}

/// One version's install-time metadata, from the package.json the
/// registry serves for it: what it depends on (regular, optional, and
/// peer — `peerDependenciesMeta` marking the optional peers out), its
/// unpacked size, whether it restricts `os` or `cpu` (a platform
/// binary).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionMeta {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub dependencies: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub optional_dependencies: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub peer_dependencies: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub peer_optional: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unpacked_size: Option<u64>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub platform: bool,
}

impl VersionMeta {
    /// The install-time dependency specs: regular + optional + non-optional
    /// peer, sorted by package name (what `npm install` resolves).
    pub fn install_deps(&self) -> Vec<(String, String)> {
        self.peer_dependencies
            .iter()
            .filter(|(n, _)| !self.peer_optional.contains(*n))
            .map(|(n, r)| (n.clone(), r.clone()))
            .chain(
                self.dependencies
                    .iter()
                    .map(|(n, r)| (n.clone(), r.clone())),
            )
            .chain(
                self.optional_dependencies
                    .iter()
                    .map(|(n, r)| (n.clone(), r.clone())),
            )
            .collect()
    }
}

impl Packument {
    pub fn from_json(text: &str) -> Result<Packument, String> {
        serde_json::from_str(text).map_err(|e| format!("packument: {e}"))
    }

    /// The registry document (`https://registry.npmjs.org/<name>` — what
    /// `npm install` itself resolves against) as a packument: the `time`
    /// map, every version, and each version's install-time metadata
    /// (`dependencies`, `optionalDependencies`, `peerDependencies` with
    /// `peerDependenciesMeta`, `dist.unpackedSize`, an `os`/`cpu`
    /// restriction). The FULL doc is the fetch's source — npm view's
    /// multi-version output omits sizes and os/cpu and drops versions
    /// that define none of the fields asked for, which left the
    /// platform and size rules dead (found on the first default-spec
    /// fetch, exp184).
    pub fn from_doc(
        doc: &serde_json::Value,
        fetched_at: Option<String>,
    ) -> Result<Packument, String> {
        let name = doc
            .get("name")
            .and_then(|x| x.as_str())
            .ok_or("registry doc: no `name`")?
            .to_string();
        let time = doc
            .get("time")
            .and_then(|x| x.as_object())
            .map(|m| {
                m.iter()
                    .filter_map(|(k, t)| Some((k.clone(), t.as_str()?.to_string())))
                    .collect()
            })
            .ok_or("registry doc: no `time` map")?;
        let versions_obj = doc.get("versions").and_then(|x| x.as_object());
        let versions_obj = match versions_obj {
            Some(v) => v,
            // An UNPUBLISHED package's tombstone: a 200 with a `time`
            // that holds an `unpublished` entry and no `versions` map.
            // Absence, not a broken fetch — the planner skips and
            // records it like any other 404.
            None => {
                if doc.get("time").and_then(|t| t.get("unpublished")).is_some() {
                    return Err(format!(
                        "{} (unpublished tombstone: {})",
                        crate::registry::NOT_FOUND,
                        doc.get("name").and_then(|x| x.as_str()).unwrap_or("?")
                    ));
                }
                return Err("registry doc: no `versions` map".into());
            }
        };
        let mut versions: Vec<String> = versions_obj.keys().cloned().collect();
        versions.sort();
        let mut deps = BTreeMap::new();
        for (version, obj) in versions_obj {
            let maps = |key: &str| -> BTreeMap<String, String> {
                obj.get(key)
                    .and_then(|x| x.as_object())
                    .map(|m| {
                        m.iter()
                            .filter_map(|(k, r)| Some((k.clone(), r.as_str()?.to_string())))
                            .collect()
                    })
                    .unwrap_or_default()
            };
            let peer_optional: BTreeSet<String> = obj
                .get("peerDependenciesMeta")
                .and_then(|x| x.as_object())
                .map(|m| {
                    m.iter()
                        .filter(|(_, meta)| {
                            meta.get("optional").and_then(|o| o.as_bool()) == Some(true)
                        })
                        .map(|(k, _)| k.clone())
                        .collect()
                })
                .unwrap_or_default();
            let strlist = |key: &str| -> Vec<String> {
                obj.get(key)
                    .and_then(|x| x.as_array())
                    .map(|a| {
                        a.iter()
                            .filter_map(|x| x.as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_default()
            };
            deps.insert(
                version.clone(),
                VersionMeta {
                    dependencies: maps("dependencies"),
                    optional_dependencies: maps("optionalDependencies"),
                    peer_dependencies: maps("peerDependencies"),
                    peer_optional,
                    unpacked_size: obj
                        .get("dist")
                        .and_then(|d| d.get("unpackedSize"))
                        .and_then(|s| s.as_u64()),
                    platform: !strlist("os").is_empty() || !strlist("cpu").is_empty(),
                },
            );
        }
        Ok(Packument {
            name,
            fetched_at,
            time,
            versions,
            deps,
            deps_fetched: true,
            absent: false,
        })
    }

    /// Whether this snapshot holds per-version metadata; a dependency
    /// rule needs it and fails loud without it.
    pub fn has_deps(&self) -> bool {
        self.deps_fetched
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

/// How long after its own publish time a version's dependencies may have
/// been published and still install: the fallback cutoff when the exact
/// one resolves nothing (a monorepo release publishes its packages seconds
/// to minutes apart, so an exact pin on a sibling is often LATER than the
/// pinning package; 2026-11-04 default-spec build: ~5,450 of ~5,550
/// date-blocked installs were within the hour, 98% within 10 minutes).
pub const GRACE_SECS: i64 = 3600;

/// `published` (a normalized UTC time) plus [`GRACE_SECS`]: the cutoff the
/// installer and the planner retry with when the exact one resolves
/// nothing. None for a time [`normalize_time`] rejects.
pub fn grace_cutoff(published: &str) -> Option<String> {
    let t = normalize_time(published)?;
    let num = |a: usize, b: usize| t[a..b].parse::<i64>().ok();
    let (y, mo, d) = (num(0, 4)?, num(5, 7)?, num(8, 10)?);
    let secs = num(11, 13)? * 3600 + num(14, 16)? * 60 + num(17, 19)? + GRACE_SECS;
    let days = days_from_civil(y, mo, d) + secs.div_euclid(86_400);
    let secs = secs.rem_euclid(86_400);
    let (y, mo, d) = civil_from_days(days);
    Some(format!(
        "{y:04}-{mo:02}-{d:02}T{:02}:{:02}:{:02}{}",
        secs / 3600,
        secs / 60 % 60,
        secs % 60,
        &t[19..]
    ))
}

/// Seconds since 1970-01-01T00:00:00Z of a UTC time [`normalize_time`]
/// accepts (the milliseconds dropped).
pub(crate) fn epoch_secs(t: &str) -> Option<i64> {
    let t = normalize_time(t)?;
    let num = |a: usize, b: usize| t[a..b].parse::<i64>().ok();
    let days = days_from_civil(num(0, 4)?, num(5, 7)?, num(8, 10)?);
    Some(days * 86_400 + num(11, 13)? * 3600 + num(14, 16)? * 60 + num(17, 19)?)
}

/// [`VersionPolicy::MinDwell`]: highest first.
fn min_dwell(
    stable: &[(Semver, &str)],
    days: u32,
    published: &dyn Fn(&str) -> Option<String>,
) -> Vec<(&'static str, String)> {
    let at: Vec<(&Semver, &str, Option<i64>)> = stable
        .iter()
        .map(|(s, v)| (s, *v, published(v).and_then(|t| epoch_secs(&t))))
        .collect();
    let mut kept: Vec<(&Semver, &str)> = at
        .iter()
        .filter(|(s, _, t)| {
            let Some(t) = *t else {
                return false;
            };
            let replaced = at
                .iter()
                .filter(|(w, _, wt)| w.major == s.major && *w > *s && wt.is_some_and(|wt| wt >= t))
                .filter_map(|(_, _, wt)| *wt)
                .min();
            replaced.is_none_or(|r| r - t >= i64::from(days) * 86_400)
        })
        .map(|(s, v, _)| (*s, *v))
        .collect();
    kept.sort_by(|a, b| b.0.cmp(a.0));
    kept.into_iter()
        .map(|(_, v)| ("dwell", v.to_string()))
        .collect()
}

/// Days since 1970-01-01 of a proleptic Gregorian date (Hinnant's
/// `days_from_civil`).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The inverse of [`days_from_civil`].
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
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
    /// Every release of the 12 months before the as-of date (`recent`),
    /// plus the newest release of every older line (a major; a 0.x
    /// minor) — the unlimited `year-all+major`: the density policy the
    /// default spec resolves to.
    YearAllMajor,
    /// The same, with the year's releases capped at the `recent` newest
    /// and the older lines capped at the `lines` most recent — the
    /// `year-last<N>+major<M>` policy.
    YearLast { recent: usize, lines: usize },
    /// The newest `n` stable versions, however old the package's last
    /// release was — for packages that publish hundreds of releases a
    /// year (the AWS SDKs).
    Latest(usize),
    /// Every stable release published in the `months` before the as-of
    /// date, plus the newest release (so a package that went quiet still
    /// holds one version) — `recent-<N>mo`, for packages that release
    /// so often that the version an app bundled is whatever was current
    /// when it was built (the AWS SDKs).
    Recent { months: u32 },
    /// Every `k`-th stable release counting back from the newest
    /// (`every-<K>`): depth spread evenly over the whole history, so no
    /// release is more than `k - 1` releases from an indexed one (exp184
    /// drift: a release keeps ~70% of its units 5 releases apart, ~10–90%
    /// at 20). With `max`, the newest `max` of them.
    Every(usize),
    /// Every stable release NOT replaced by a higher release of its own
    /// major line within `days` of its publish (`min-dwell-<N>d`): a
    /// release fixed by a fast follow-up was barely installed (exp184:
    /// such versions draw ~1-5% of the downloads of the others), so the
    /// rest carry most of the real use. Each major's newest always stays.
    /// For packages that release on a schedule (the AWS SDKs) nearly
    /// every release is "replaced quickly" — use `every-<K>` there.
    MinDwell { days: u32 },
    /// Nothing but the pins (`--pinned`: the bundle's readers named them).
    PinsOnly,
}

impl VersionPolicy {
    /// The policy's name as a spec spells it (the sidecar's plan record).
    pub fn name(&self) -> String {
        match self {
            VersionPolicy::AsOf => "as-of".to_string(),
            VersionPolicy::YearAllMinor => "year-all+minor".to_string(),
            VersionPolicy::YearAllMajor => "year-all+major".to_string(),
            VersionPolicy::YearLast { recent, lines } => {
                format!("year-last{recent}+major{lines}")
            }
            VersionPolicy::Latest(1) => "latest".to_string(),
            VersionPolicy::Latest(n) => format!("latest-{n}"),
            VersionPolicy::Recent { months } => format!("recent-{months}mo"),
            VersionPolicy::Every(k) => format!("every-{k}"),
            VersionPolicy::MinDwell { days } => format!("min-dwell-{days}d"),
            VersionPolicy::PinsOnly => "pins".to_string(),
        }
    }

    pub fn from_name(name: &str) -> Option<VersionPolicy> {
        if let Some(n) = name.strip_prefix("latest") {
            return Some(match n {
                "" => VersionPolicy::Latest(1),
                _ => {
                    let n = n.strip_prefix('-')?;
                    VersionPolicy::Latest(n.parse().ok()?)
                }
            });
        }
        if let Some(rest) = name.strip_prefix("min-dwell-") {
            let days: u32 = rest.strip_suffix('d')?.parse().ok()?;
            return (days > 0).then_some(VersionPolicy::MinDwell { days });
        }
        if let Some(rest) = name.strip_prefix("every-") {
            let k: usize = rest.parse().ok()?;
            return (k > 0).then_some(VersionPolicy::Every(k));
        }
        if let Some(rest) = name.strip_prefix("recent-") {
            let months: u32 = rest.strip_suffix("mo")?.parse().ok()?;
            return (months > 0).then_some(VersionPolicy::Recent { months });
        }
        if let Some(rest) = name.strip_prefix("year-last") {
            let (recent, lines) = rest.split_once("+major")?;
            let lines = match lines {
                "" => 4,
                _ => lines.parse().ok()?,
            };
            return Some(VersionPolicy::YearLast {
                recent: recent.parse().ok()?,
                lines,
            });
        }
        match name {
            "as-of" => Some(VersionPolicy::AsOf),
            "year-all+minor" => Some(VersionPolicy::YearAllMinor),
            "year-all+major" => Some(VersionPolicy::YearAllMajor),
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
    let since = year_before(as_of);
    if since.is_empty() {
        return Vec::new();
    }
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

/// `months` before a normalised time: the same day-of-month and instant,
/// `months` calendar months earlier (a day the earlier month lacks still
/// orders correctly as a string bound).
fn months_before(as_of: &str, months: u32) -> String {
    let (Some(y), Some(m)) = (
        as_of.get(..4).and_then(|y| y.parse::<i64>().ok()),
        as_of.get(5..7).and_then(|m| m.parse::<i64>().ok()),
    ) else {
        return String::new();
    };
    let total = y * 12 + (m - 1) - i64::from(months);
    let (y, m) = (total.div_euclid(12), total.rem_euclid(12) + 1);
    format!("{y:04}-{m:02}{}", &as_of[7..])
}

/// `recent-<N>mo` over stable versions with their publish times: every
/// release of the window, highest first, else the newest alone.
fn recent_months(
    stable: &[(Semver, &str)],
    as_of: &str,
    months: u32,
    published: &dyn Fn(&str) -> Option<String>,
) -> Vec<(&'static str, String)> {
    let since = months_before(as_of, months);
    let mut sorted: Vec<&(Semver, &str)> = stable.iter().collect();
    sorted.sort_by(|a, b| b.0.cmp(&a.0));
    let out: Vec<(&'static str, String)> = sorted
        .iter()
        .filter(|(_, v)| {
            published(v).is_some_and(|t| t.as_str() > since.as_str() && t.as_str() <= as_of)
        })
        .map(|(_, v)| ("recent", (*v).to_string()))
        .collect();
    if !out.is_empty() {
        return out;
    }
    sorted
        .first()
        .map(|(_, v)| vec![("latest", (*v).to_string())])
        .unwrap_or_default()
}

/// Twelve months before a normalised time: the same instant a year
/// earlier.
fn year_before(as_of: &str) -> String {
    months_before(as_of, 12)
}

/// Every release of the last year plus the newest of every older line
/// (`line` = major, or a 0.x minor), the year's releases capped at
/// `recent_cap` (None: unlimited) and the lines at `lines_cap` (None:
/// unlimited): (role, version), highest first. The density policy
/// tail_pins.py measured (experiments/101-index-coverage).
fn year_plus_lines(
    stable: &[(Semver, &str)],
    as_of: &str,
    published: &dyn Fn(&str) -> Option<String>,
    recent_cap: Option<usize>,
    lines_cap: Option<usize>,
) -> Vec<(&'static str, String)> {
    let since = year_before(as_of);
    let in_year =
        |v: &str| published(v).is_some_and(|t| t.as_str() > since.as_str() && t.as_str() <= as_of);
    let mut sorted: Vec<&(Semver, &str)> = stable.iter().collect();
    sorted.sort_by(|a, b| b.0.cmp(&a.0));
    let line = |s: &Semver| {
        if s.major == 0 {
            (0, s.minor)
        } else {
            (s.major, 0)
        }
    };
    let mut out = Vec::new();
    let mut kept: BTreeSet<&str> = BTreeSet::new();
    let mut recent = 0;
    for (_, v) in sorted.iter() {
        if in_year(v) && recent_cap.is_none_or(|cap| recent < cap) {
            recent += 1;
            kept.insert(*v);
            out.push(("recent", (*v).to_string()));
        }
    }
    let mut lines_seen: BTreeSet<(u64, u64)> = BTreeSet::new();
    for (s, v) in sorted.iter() {
        let line = line(s);
        let newest_of_line = lines_seen.insert(line);
        if newest_of_line
            && lines_cap.is_none_or(|cap| lines_seen.len() <= cap)
            && !kept.contains(*v)
        {
            out.push(("major", (*v).to_string()));
        }
    }
    out
}
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
        VersionPolicy::YearAllMajor => year_plus_lines(versions, as_of, &published, None, None),
        VersionPolicy::YearLast { recent, lines } => {
            year_plus_lines(versions, as_of, &published, Some(recent), Some(lines))
        }
        VersionPolicy::Latest(n) => {
            let mut sorted: Vec<&(Semver, &str)> = versions.iter().collect();
            sorted.sort_by(|a, b| b.0.cmp(&a.0));
            sorted
                .into_iter()
                .take(n)
                .map(|(_, v)| ("latest", (*v).to_string()))
                .collect()
        }
        VersionPolicy::Recent { months } => recent_months(versions, as_of, months, &published),
        VersionPolicy::Every(k) => {
            let mut sorted: Vec<&(Semver, &str)> = versions.iter().collect();
            sorted.sort_by(|a, b| b.0.cmp(&a.0));
            sorted
                .into_iter()
                .step_by(k)
                .map(|(_, v)| ("every", (*v).to_string()))
                .collect()
        }
        VersionPolicy::MinDwell { days } => min_dwell(versions, days, &published),
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
