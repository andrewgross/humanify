use super::*;

#[test]
fn a_plant_counts_only_when_it_is_detected_as_a_differing_case() {
    assert!(plant_detected(false, "cases 120, identical 119, differing 1").is_ok());
    assert!(plant_detected(false, "differing 12 of 120").is_ok());
    // Exit 0: the plant went unnoticed — the check is vacuous.
    let e = plant_detected(true, "differing 1").unwrap_err();
    assert!(e.contains("NOT detected"), "{e}");
    // Nonzero for another reason (an unknown plant, a crash) proves nothing.
    assert!(plant_detected(false, "error: unknown plant x").is_err());
    assert!(plant_detected(false, "differing 0").is_err());
}

#[test]
fn the_launches_are_the_two_format_check_runs() {
    let repo = Path::new("/r");
    let [check, plant] = launches(repo);
    assert_eq!(
        check.describe(),
        "/r/target/release/humanify-ag format-check /r/test/parity/format-goldens.json"
    );
    assert_eq!(
        plant.describe(),
        "/r/target/release/humanify-ag format-check /r/test/parity/format-goldens.json --plant rust-number-format --show 0"
    );
}
