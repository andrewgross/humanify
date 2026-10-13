//! As-of version selection, from a RECORDED packument (no network):
//! `testdata/ms.packument.json` is `npm view ms time versions --json` as of
//! 2026-10-11.

use super::asof::{Packument, VersionPolicy, normalize_time, parse_as_of, select_versions};

fn ms() -> Packument {
    Packument::from_json(include_str!("../testdata/ms.packument.json")).expect("fixture")
}

fn picked(as_of: &str, pins: &[&str]) -> Vec<(String, String)> {
    let pins: Vec<String> = pins.iter().map(|s| (*s).to_string()).collect();
    select_versions(
        &ms(),
        &parse_as_of(as_of).expect("date"),
        &pins,
        VersionPolicy::AsOf,
    )
    .selected
    .into_iter()
    .map(|s| (s.role, s.version))
    .collect()
}

fn pair(role: &str, v: &str) -> (String, String) {
    (role.to_string(), v.to_string())
}

#[test]
fn a_date_means_the_end_of_that_utc_day() {
    assert_eq!(
        parse_as_of("2020-12-08").expect("date"),
        "2020-12-08T23:59:59.999Z"
    );
    assert_eq!(
        parse_as_of("2020-12-08T13:00:00Z").expect("time"),
        "2020-12-08T13:00:00.000Z"
    );
    assert!(parse_as_of("12/08/2020").is_err());
    assert_eq!(
        normalize_time("2017-03-19T21:43:15.128Z").as_deref(),
        Some("2017-03-19T21:43:15.128Z")
    );
}

#[test]
fn the_latest_today_is_the_highest_stable_version_with_its_neighbours() {
    assert_eq!(
        picked("2026-10-11", &[]),
        vec![
            pair("latest", "2.1.3"),
            pair("patch", "2.1.2"),
            pair("major-1", "1.0.0"),
            pair("major-2", "0.7.3"),
        ]
    );
}

#[test]
fn an_as_of_date_before_a_release_excludes_it() {
    // 2.1.3 was published 2020-12-08T13:54:35Z.
    let plan = select_versions(
        &ms(),
        &parse_as_of("2020-12-01").expect("date"),
        &[],
        VersionPolicy::AsOf,
    );
    let versions: Vec<&str> = plan.selected.iter().map(|s| s.version.as_str()).collect();
    assert_eq!(versions, ["2.1.2", "2.1.1", "1.0.0", "0.7.3"]);
    assert!(
        plan.refused
            .iter()
            .any(|r| r.version == "2.1.3" && r.reason == "published-after-as-of"),
        "{:?}",
        plan.refused
    );
    // The same day, an hour after it: admitted.
    assert_eq!(
        picked("2020-12-08T14:00:00Z", &[])[0],
        pair("latest", "2.1.3")
    );
}

#[test]
fn every_selected_version_was_published_on_or_before_the_date() {
    let as_of = parse_as_of("2017-06-01").expect("date");
    let plan = select_versions(&ms(), &as_of, &[], VersionPolicy::AsOf);
    assert_eq!(plan.selected[0].version, "2.0.0");
    assert!(plan.selected.iter().all(|s| s.published <= as_of));
}

#[test]
fn prereleases_are_never_chosen_by_the_policy() {
    // 3.0.0-canary.* and 4.0.0-nightly.* exist today; 2.1.3 stays latest.
    assert_eq!(picked("2026-10-11", &[])[0], pair("latest", "2.1.3"));
}

#[test]
fn a_pinned_version_is_indexed_only_if_it_existed_at_the_date() {
    let got = picked("2019-01-01", &["0.7.1"]);
    assert!(got.contains(&pair("pinned", "0.7.1")), "{got:?}");
    let plan = select_versions(
        &ms(),
        &parse_as_of("2019-01-01").expect("date"),
        &["2.1.3".to_string(), "9.9.9".to_string()],
        VersionPolicy::AsOf,
    );
    assert!(plan.selected.iter().all(|s| s.version != "2.1.3"));
    let reasons: Vec<(&str, &str)> = plan
        .refused
        .iter()
        .map(|r| (r.version.as_str(), r.reason.as_str()))
        .collect();
    assert!(
        reasons.contains(&("2.1.3", "published-after-as-of")),
        "{reasons:?}"
    );
    assert!(
        reasons.contains(&("9.9.9", "not-in-registry")),
        "{reasons:?}"
    );
}

#[test]
fn an_unpublished_version_cannot_be_indexed() {
    let p = Packument::from_json(
        r#"{"name":"x","time":{"created":"2020-01-01T00:00:00.000Z","1.0.0":"2020-01-01T00:00:00.000Z","1.1.0":"2020-02-01T00:00:00.000Z"},"versions":["1.0.0"]}"#,
    )
    .expect("json");
    let plan = select_versions(
        &p,
        &parse_as_of("2021-01-01").expect("date"),
        &[],
        VersionPolicy::AsOf,
    );
    let versions: Vec<&str> = plan.selected.iter().map(|s| s.version.as_str()).collect();
    assert_eq!(versions, ["1.0.0"]);
    assert!(
        plan.refused
            .iter()
            .any(|r| r.version == "1.1.0" && r.reason == "unpublished")
    );
}

#[test]
fn for_zero_x_the_minor_plays_the_major() {
    let p = Packument::from_json(
        r#"{"name":"z","time":{"0.1.0":"2020-01-01T00:00:00.000Z","0.1.1":"2020-01-02T00:00:00.000Z","0.2.0":"2020-02-01T00:00:00.000Z","0.3.0":"2020-03-01T00:00:00.000Z","0.3.1":"2020-03-02T00:00:00.000Z"},"versions":["0.1.0","0.1.1","0.2.0","0.3.0","0.3.1"]}"#,
    )
    .expect("json");
    let plan = select_versions(
        &p,
        &parse_as_of("2021-01-01").expect("date"),
        &[],
        VersionPolicy::AsOf,
    );
    let got: Vec<(&str, &str)> = plan
        .selected
        .iter()
        .map(|s| (s.role.as_str(), s.version.as_str()))
        .collect();
    assert_eq!(
        got,
        [
            ("latest", "0.3.1"),
            ("patch", "0.3.0"),
            ("major-1", "0.2.0"),
            ("major-2", "0.1.1")
        ]
    );
}

#[test]
fn year_all_plus_minor_is_every_recent_release_and_each_older_minors_latest_patch() {
    let plan = select_versions(
        &ms(),
        &parse_as_of("2017-12-31").expect("date"),
        &[],
        VersionPolicy::YearAllMinor,
    );
    let got: Vec<(String, String)> = plan
        .selected
        .into_iter()
        .map(|s| (s.role, s.version))
        .collect();
    assert_eq!(
        got,
        vec![
            // published 2017-01-01 .. 2017-12-31
            pair("recent", "2.1.1"),
            pair("recent", "2.1.0"),
            pair("recent", "2.0.0"),
            pair("recent", "1.0.0"),
            pair("recent", "0.7.3"),
            // older minors: their latest patch
            pair("minor", "0.6.2"),
            pair("minor", "0.5.1"),
            pair("minor", "0.4.0"),
            pair("minor", "0.3.0"),
            pair("minor", "0.2.0"),
            pair("minor", "0.1.0"),
        ]
    );
}
