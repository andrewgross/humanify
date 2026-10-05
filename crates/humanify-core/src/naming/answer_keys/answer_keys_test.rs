use humanify_model::llm::Renames;

use super::{key_answer, normalize};

fn answer(pairs: &[(&str, &str)]) -> Renames {
    Renames::from_entries(
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), Some(v.to_string()))),
    )
}

fn ids(names: &[&str]) -> Vec<String> {
    names.iter().map(|s| s.to_string()).collect()
}

const NO_NAMES: &dyn Fn(&str) -> bool = &|_| false;

#[test]
fn the_normal_form_reads_a_dollar_underscore_run_as_one_separator() {
    assert_eq!(normalize("y$_").as_deref(), Some("y$"));
    assert_eq!(normalize("y$").as_deref(), Some("y$"));
    assert_eq!(normalize("y$$_").as_deref(), Some("y$"));
    assert_eq!(normalize(" $95").as_deref(), Some("$95"));
    assert_eq!(normalize("a_b").as_deref(), Some("a$b"));
    assert_ne!(
        normalize("ab"),
        normalize("a_b"),
        "a separator never vanishes"
    );
    assert_eq!(normalize("_"), None, "separators alone are never matched");
    assert_eq!(normalize("$_$"), None);
}

/// Both shapes the census found (`y$_` keyed `y$` and `y$$_`) land on
/// the asked id, re-keyed, and are reported; exact keys are untouched.
#[test]
fn a_mangled_key_lands_on_its_one_asked_identifier() {
    let keyed = key_answer(
        &answer(&[
            ("y$", "messageHandler"),
            ("CbH", "errorHandler"),
            ("V$$_", "isEnded"),
        ]),
        &ids(&["y$_", "CbH", "V$_"]),
        NO_NAMES,
    );
    assert_eq!(keyed.renames.get("y$_"), Some("messageHandler"));
    assert_eq!(keyed.renames.get("V$_"), Some("isEnded"));
    assert_eq!(keyed.renames.get("CbH"), Some("errorHandler"));
    assert_eq!(
        keyed.tolerant,
        [
            ("y$_".to_string(), "y$".to_string()),
            ("V$_".to_string(), "V$$_".to_string())
        ]
    );
    assert_eq!(keyed.answer_key("y$_"), Some("y$"));
    assert_eq!(keyed.answer_key("CbH"), None);
    assert!(keyed.stray.is_empty());
}

/// Two asked ids that read the same once normalised: the key could be
/// either, so it is neither — stray, and both stay unanswered.
#[test]
fn a_key_two_asked_ids_could_own_is_matched_to_neither() {
    let keyed = key_answer(&answer(&[("a$", "first")]), &ids(&["a$_", "a_$"]), NO_NAMES);
    assert!(keyed.tolerant.is_empty());
    assert_eq!(keyed.stray, ["a$"]);
    assert!(!keyed.answered("a$_") && !keyed.answered("a_$"));
}

/// Two keys that both read as one asked id: never two keys to one id.
#[test]
fn two_keys_for_one_asked_id_are_both_stray() {
    let keyed = key_answer(
        &answer(&[("y$", "first"), ("y$$_", "second")]),
        &ids(&["y$_"]),
        NO_NAMES,
    );
    assert!(keyed.tolerant.is_empty());
    assert_eq!(keyed.stray, ["y$", "y$$_"]);
    assert!(!keyed.answered("y$_"));
}

/// An exact key always wins: a near-miss key for an id the answer already
/// keyed exactly is stray, never a second answer.
#[test]
fn an_exactly_answered_id_takes_no_tolerant_key() {
    let keyed = key_answer(
        &answer(&[("y$_", "exact"), ("y$", "other")]),
        &ids(&["y$_"]),
        NO_NAMES,
    );
    assert_eq!(keyed.renames.get("y$_"), Some("exact"));
    assert!(keyed.tolerant.is_empty());
    assert_eq!(keyed.stray, ["y$"]);
}

/// A key that is some other binding's name is the model naming THAT
/// binding — never read as a misspelling of the asked id.
#[test]
fn a_key_that_is_another_name_in_the_program_is_stray() {
    let is_name = |k: &str| k == "y$";
    let keyed = key_answer(&answer(&[("y$", "handler")]), &ids(&["y$_"]), &is_name);
    assert!(keyed.tolerant.is_empty());
    assert_eq!(keyed.stray, ["y$"]);
}

/// A completely different key is stray; case is NOT tolerated (minified
/// code uses `a` and `A` as different bindings).
#[test]
fn unrelated_and_case_changed_keys_are_stray() {
    let keyed = key_answer(
        &answer(&[("getValue", "x"), ("XIU", "y")]),
        &ids(&["_", "XIu"]),
        NO_NAMES,
    );
    assert!(keyed.tolerant.is_empty());
    assert_eq!(keyed.stray, ["getValue", "XIU"]);
}
