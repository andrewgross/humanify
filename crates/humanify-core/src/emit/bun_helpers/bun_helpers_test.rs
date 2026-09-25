//! Ported from src/shared/bun-helpers.test.ts (`identifyBunLazyInit`),
//! plus the regex semantics the port hand-matches.

use super::identify_bun_lazy_init as lazy;

#[test]
fn identifies_the_minified_lazy_init_helper() {
    assert_eq!(
        lazy("var L=(I,A,q)=>(A&&(q=A(A=0)),q);").as_deref(),
        Some("L")
    );
}

#[test]
fn returns_none_for_non_lazy_init_code() {
    assert_eq!(lazy("var x = () => 1;"), None);
}

#[test]
fn identifies_the_beautified_helper_in_a_comma_declaration() {
    let src = "var a = 1;\nvar first = 2,\n  initLazy = (fn, res) => () => (fn && (res = fn(fn = 0)), res);";
    // The binding regex takes the LEFTMOST `,NAME =`/`var NAME =` after the
    // last `;` — the declaration's first declarator, as the TS does.
    assert_eq!(lazy(src).as_deref(), Some("first"));
}

#[test]
fn the_backreference_must_repeat_group_one_verbatim() {
    assert_eq!(lazy("var f = (a, r) => (a && (r = b(a = 0)), r);"), None);
    assert_eq!(lazy("var f = (a, r) => (a && (r = a(b = 0)), r);"), None);
}

#[test]
fn a_match_may_start_inside_a_word_run() {
    // `xa && (r = a(a = 0))`: group 1 = `a`, starting inside `xa`.
    assert_eq!(
        lazy("var g = (xa, r) => (xa && (r = a(a = 0)), r);").as_deref(),
        Some("g")
    );
}

#[test]
fn the_first_matching_helper_wins() {
    let src = "var one = (a, r) => (a && (r = a(a = 0)), r);\nvar two = (b, s) => (b && (s = b(b = 0)), s);";
    assert_eq!(lazy(src).as_deref(), Some("one"));
}
