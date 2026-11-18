use super::{Card, Located, Matcher, score};

fn f(pair: Option<u32>, start: u32) -> Located {
    Located {
        pair,
        order: u64::from(start),
    }
}

fn card(fresh: &[Located], prior: &[Located], matcher: &Matcher) -> Card {
    score(fresh, prior, matcher)
}

#[test]
fn an_unchanged_function_matched_to_its_true_partner_is_correct() {
    let fresh = [f(Some(0), 5), f(Some(1), 50), f(Some(2), 90), f(None, 120)];
    let prior = [f(Some(1), 7), f(Some(0), 60), f(Some(2), 80), f(None, 130)];
    let matcher = Matcher {
        exact: vec![(1, 0), (2, 1), (3, 3)],
        close: vec![(2, 2)],
        ambiguous: [].into(),
    };
    let c = card(&fresh, &prior, &matcher);
    assert_eq!(c.unchanged, 3);
    assert_eq!(c.correct, 1);
    assert_eq!(c.wrong, 1);
    assert_eq!(
        (c.missed_close, c.missed_ambiguous, c.missed_unmatched),
        (1, 0, 0)
    );
    assert_eq!((c.other_matched, c.other_unmatched), (1, 0));
}

#[test]
fn functions_inside_one_unchanged_unit_pair_by_their_order() {
    let fresh = [f(Some(4), 10), f(Some(4), 20), f(Some(4), 30)];
    let prior = [f(Some(4), 100), f(Some(4), 200), f(Some(4), 300)];
    let matcher = Matcher {
        exact: vec![(0, 0), (2, 1)],
        close: vec![],
        ambiguous: [2].into(),
    };
    let c = card(&fresh, &prior, &matcher);
    assert_eq!((c.unchanged, c.correct, c.wrong), (3, 1, 1));
    assert_eq!(c.missed_ambiguous, 1);
}

#[test]
fn a_unit_with_another_number_of_functions_on_each_side_is_not_judged() {
    let fresh = [f(Some(4), 10), f(Some(4), 20)];
    let prior = [f(Some(4), 100)];
    let matcher = Matcher {
        exact: vec![(0, 0)],
        close: vec![],
        ambiguous: [].into(),
    };
    let c = card(&fresh, &prior, &matcher);
    assert_eq!(c.unchanged, 0);
    assert_eq!(c.not_judged, 2);
}

#[test]
fn a_close_match_to_another_function_is_counted_apart() {
    let fresh = [f(Some(0), 5), f(Some(1), 50)];
    let prior = [f(Some(0), 7), f(Some(1), 60)];
    let matcher = Matcher {
        exact: vec![],
        close: vec![(0, 1)],
        ambiguous: [].into(),
    };
    let c = card(&fresh, &prior, &matcher);
    assert_eq!((c.missed_close, c.missed_close_other), (0, 2));
    assert_eq!(c.missed_unmatched, 0);
}
