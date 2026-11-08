//! The forms registry: every form is listed once under a unique name, the
//! persisted stamps hold their values, and the forms that share a walk's
//! era move together.

use std::collections::HashSet;

use super::forms::*;
use super::identity_transforms::{Transform, TransformSet};

#[test]
fn every_form_is_listed_once_under_a_unique_name() {
    let names: HashSet<&str> = FORMS.iter().map(|f| f.name).collect();
    assert_eq!(names.len(), FORMS.len());
}

#[test]
fn the_persisted_stamps_keep_their_values() {
    // An index shard, a vendor manifest and a split ledger are keyed by
    // these; a change is a new era, never a refactor.
    assert_eq!(MATCH_KEY.version, 5); // exp200: minifier style

    assert_eq!(STATEMENT.version, 3);
    assert_eq!(KPI_STATEMENT.stamp(), "kpi-statement/2"); // the eval scorerVersion
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
        [
            "match-key",
            "units",
            "vendor-content-key",
            "statement",
            "kpi-statement"
        ]
    );
}

#[test]
fn derived_forms_move_with_their_base() {
    // The in-run variants and the carry's content key ARE the match key's
    // walk; a match-key era is theirs too.
    assert_eq!(MATCH_KEY_PRIVATES_BLINDED.version, MATCH_KEY.version);
    assert_eq!(MATCH_KEY_PRIVATES_VERBATIM.version, MATCH_KEY.version);
    assert_eq!(CARRY_CONTENT.version, MATCH_KEY.version);
    assert_eq!(CARRY_CONTENT.transforms, MATCH_KEY.transforms);
    assert_eq!(CARRY_CONTENT.privates, MATCH_KEY.privates);
    assert_eq!(MATCH_KEY_PRIVATES_VERBATIM.transforms, MATCH_KEY.transforms);
}

#[test]
fn the_index_forms_keep_the_settings_they_were_stamped_with() {
    // exp200 split the library index's forms from the match key: every
    // built index carries `units/3` and `vendor-content-key/4`, computed
    // under the v4 match key (wrapper spelling only, privates verbatim
    // before the units blind them). A match-key era must not re-key them;
    // re-fingerprinting the index is its own version bump and rebuild.
    let wrapper_only = TransformSet::of(&[Transform::WrapperSpelling]);
    for form in [&LIBRARY_UNITS, &VENDOR_CONTENT] {
        assert_eq!(form.transforms, wrapper_only, "{}", form.name);
        assert_eq!(form.locals, Locals::Slots, "{}", form.name);
        assert_eq!(form.strings, Strings::Blurred, "{}", form.name);
        assert_eq!(form.numbers, Numbers::Exact, "{}", form.name);
    }
    assert_eq!(LIBRARY_UNITS.privates, Privates::Blinded);
    assert_eq!(VENDOR_CONTENT.privates, Privates::Verbatim);
    assert_eq!(VENDOR_CONTENT.free_names, FreeNames::SlottedUnlessGlobal);
    // The library units differ from the twin gate's masked stream only in
    // name, stamp and the match key's exp200 transform.
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
            transforms: wrapper_only,
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
                assert!(form.transforms.is_empty(), "{}", form.name);
            }
        }
    }
    // The identity key is byte-exact: no identity transform.
    assert!(IDENTITY_KEY.transforms.is_empty() && RENAME_SIGNATURE.transforms.is_empty());
}

#[test]
fn every_transform_a_form_lists_is_applied_by_its_walk() {
    for form in FORMS {
        for t in form.transforms.iter() {
            assert!(
                t.spec().walks.contains(&form.walk),
                "{} lists {} but its walk does not apply it",
                form.name,
                t.spec().name
            );
        }
    }
}
