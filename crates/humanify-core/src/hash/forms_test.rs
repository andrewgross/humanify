//! The forms registry: every form is listed once under a unique name, the
//! persisted stamps hold their values, and the forms that share a walk's
//! era move together.

use std::collections::HashSet;

use super::forms::*;

#[test]
fn every_form_is_listed_once_under_a_unique_name() {
    let names: HashSet<&str> = FORMS.iter().map(|f| f.name).collect();
    assert_eq!(names.len(), FORMS.len());
}

#[test]
fn the_persisted_stamps_keep_their_values() {
    // An index shard, a vendor manifest and a split ledger are keyed by
    // these; a change is a new era, never a refactor.
    assert_eq!(MATCH_KEY.version, 4);
    assert_eq!(STATEMENT.version, 3);
    assert_eq!(LIBRARY_UNITS.stamp(), "units/3"); // the unit model moved (exp097)
    assert_eq!(VENDOR_CONTENT.stamp(), "vendor-content-key/4");
    assert_eq!(LIBRARY_LITERALS_VERSION, 1);
    let stamped: Vec<&str> = FORMS
        .iter()
        .filter(|f| f.stamped_in.is_some())
        .map(|f| f.name)
        .collect();
    assert_eq!(
        stamped,
        ["match-key", "units", "vendor-content-key", "statement"]
    );
}

#[test]
fn derived_forms_move_with_their_base() {
    // The vendor content key and the twin gate's masked stream ARE the
    // match key's walk; a match-key era is theirs too.
    assert_eq!(VENDOR_CONTENT.version, MATCH_KEY.version);
    assert_eq!(MATCH_KEY_PRIVATES_BLINDED.version, MATCH_KEY.version);
    // The library units differ from the twin gate's masked stream only in
    // name and stamp: the same settings.
    assert_eq!(
        CanonicalForm {
            name: "",
            version: 0,
            stamped_in: None,
            ..LIBRARY_UNITS
        },
        CanonicalForm {
            name: "",
            version: 0,
            stamped_in: None,
            ..MATCH_KEY_PRIVATES_BLINDED
        }
    );
}

#[test]
fn each_form_names_a_walk_that_can_produce_it() {
    for form in FORMS {
        match form.walk {
            Walk::Canonical => {
                assert_eq!(form.locals, Locals::Slots, "{}", form.name);
                assert_eq!(form.numbers, Numbers::Exact, "{}", form.name);
                assert_eq!(
                    form.property_names,
                    PropertyNames::Verbatim,
                    "{}",
                    form.name
                );
                assert_eq!(form.parent_names, ParentNames::Oxc, "{}", form.name);
                assert_ne!(form.privates, Privates::Masked, "{}", form.name);
            }
            Walk::Statement => {
                assert_eq!(form.locals, Locals::Masked, "{}", form.name);
                assert_eq!(form.property_names, PropertyNames::Masked, "{}", form.name);
            }
            Walk::AlignTokens => {
                assert_eq!(form.parent_names, ParentNames::Babel, "{}", form.name);
                assert!(!form.wrapper_spelling, "{}", form.name);
            }
        }
    }
    // The identity key is byte-exact: no wrapper-spelling rule.
    const { assert!(!IDENTITY_KEY.wrapper_spelling && !RENAME_SIGNATURE.wrapper_spelling) };
}
