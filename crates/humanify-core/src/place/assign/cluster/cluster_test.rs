//! The fresh grouping's size targets scale to the input (finding B1 of
//! the 2026-10-05 app-specific scan): the old fixed targets were Claude
//! Code's real source tree, and they stay EXACT at Claude Code's size.

use super::{
    ClusterConfig, ClusterSizing, DEFAULT_CLUSTER_CONFIG, REFERENCE_APP_LINES,
    scaled_cluster_config,
};

/// At the reference size the scaled targets ARE the old constants.
#[test]
fn the_reference_size_reproduces_the_old_targets() {
    assert_eq!(
        scaled_cluster_config(REFERENCE_APP_LINES),
        DEFAULT_CLUSTER_CONFIG
    );
}

/// A small app gets a handful of files in a few top folders — not one
/// top folder of dozens of tiny files (the fixed 40-100 window needs
/// more than 100 cuts before a second top folder appears).
#[test]
fn a_small_app_gets_proportionate_targets() {
    let c: ClusterConfig = scaled_cluster_config(5_000);
    assert_eq!(c.target_files, 19, "5,000 lines at the reference file size");
    assert!(c.min_top >= 1 && c.max_top <= 15, "{c:?}");
    assert!(c.min_top < c.max_top, "{c:?}");
    assert!(c.folder_window < DEFAULT_CLUSTER_CONFIG.folder_window);
    // File-level budgets are a FILE's size, not the project's: kept.
    assert_eq!(c.max_lines, DEFAULT_CLUSTER_CONFIG.max_lines);
    assert_eq!(c.min_lines, DEFAULT_CLUSTER_CONFIG.min_lines);
    assert_eq!(c.max_seg, DEFAULT_CLUSTER_CONFIG.max_seg);
    assert_eq!(c.window, DEFAULT_CLUSTER_CONFIG.window);
    // Sub-folder sizes (6-25 files) and the flat-top rule are a FOLDER's.
    assert_eq!(c.min_sub, DEFAULT_CLUSTER_CONFIG.min_sub);
    assert_eq!(c.max_sub, DEFAULT_CLUSTER_CONFIG.max_sub);
    assert_eq!(c.flat_top, DEFAULT_CLUSTER_CONFIG.flat_top);
}

/// A bundle twice Claude Code's size gets twice the files (the fixed
/// target capped it at 1,700, so its files grew instead).
#[test]
fn a_larger_app_is_not_capped_at_the_reference_file_count() {
    let c = scaled_cluster_config(2 * REFERENCE_APP_LINES);
    assert_eq!(c.target_files, 3_400);
    assert!(c.max_top > DEFAULT_CLUSTER_CONFIG.max_top);
}

/// Never a zero target, whatever the input.
#[test]
fn a_tiny_app_still_gets_valid_targets() {
    for lines in [0, 1, 30, 200] {
        let c = scaled_cluster_config(lines);
        assert!(c.target_files >= 1, "{lines}: {c:?}");
        assert!(c.min_top >= 1 && c.min_top <= c.max_top, "{lines}: {c:?}");
        assert!(c.folder_window >= 1, "{lines}: {c:?}");
    }
}

/// A small wrapper bundle's split input: `n` one-line statements.
fn small_bundle(n: usize) -> (String, crate::place::input::SplitInput) {
    let mut code = String::from("(function () {\n");
    for i in 0..n {
        code.push_str(&format!("  var item{i:02} = {i};\n"));
    }
    code.push_str("})();\n");
    let input = crate::place::input::split_input(
        &code,
        crate::toolchain::BundleLayout::SingleWrapperFunction,
    )
    .expect("a wrapper bundle");
    (code, input)
}

fn assign(code: &str, input: &crate::place::input::SplitInput, cuts: &[usize]) -> Vec<String> {
    super::assign_clustered(
        &input.body,
        Some((code, input.spans.as_slice())),
        None,
        ClusterSizing::ScaledToApp,
        cuts,
        super::ClusterNamers {
            namer: None,
            reviser: None,
        },
    )
}

/// A lazily loaded module's end is a file boundary the grouping honours
/// (the module markers' boundaries, kept when the markers do not describe
/// the whole bundle): a tiny app is one file, unless a module ends inside
/// it.
#[test]
fn a_forced_cut_after_a_module_end_is_a_file_boundary() {
    let (code, input) = small_bundle(55);
    let plain = assign(&code, &input, &[]);
    assert!(plain.iter().all(|f| *f == plain[0]), "one file: {plain:?}");
    let cut = assign(&code, &input, &[10]);
    assert_ne!(cut[10], cut[11], "the module ends at statement 10");
    assert!(cut[..=10].iter().all(|f| *f == cut[0]));
    assert!(cut[11..].iter().all(|f| *f == cut[11]));
}

/// A module that fits one file's budget is never cut inside: the seams'
/// and budgets' cuts within it are dropped (a lazily loaded module is one
/// file, as the markers record it). The code after it still gets the
/// budget cuts it needs (79 statements over the 60-statement cap).
#[test]
fn a_module_that_fits_one_file_is_never_cut_inside() {
    let (code, input) = small_bundle(120);
    let plain = assign(&code, &input, &[]);
    let cut_inside = (1..=40).any(|i| plain[i] != plain[i - 1]);
    assert!(cut_inside, "the seams alone cut inside 0..=40: {plain:?}");
    let kept = assign(&code, &input, &[40]);
    assert!(kept[..=40].iter().all(|f| *f == kept[0]), "{kept:?}");
    assert_ne!(kept[40], kept[41]);
    assert!(
        (42..120).any(|i| kept[i] != kept[i - 1]),
        "the rest is still budget-cut: {kept:?}"
    );
}

/// The fixed sizing is the replay path's (the frozen TS split calls).
#[test]
fn fixed_sizing_is_used_verbatim() {
    let mut c = DEFAULT_CLUSTER_CONFIG;
    c.target_files = 7;
    assert_eq!(ClusterSizing::Fixed(c).config_for(123_456), c);
    assert_eq!(
        ClusterSizing::ScaledToApp.config_for(REFERENCE_APP_LINES),
        DEFAULT_CLUSTER_CONFIG
    );
}
