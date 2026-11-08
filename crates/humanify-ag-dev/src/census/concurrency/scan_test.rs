//! The scanner's pieces: what is blanked, and how a scope is named (the
//! allowlist's `scope` strings depend on these names byte for byte).

use super::*;

fn scopes(src: &str) -> Vec<(String, String)> {
    let units: Vec<u16> = src.encode_utf16().collect();
    let (text, _) = strip_cfg_test(&sanitize(src), &units);
    sites_of("f.rs", &text)
        .into_iter()
        .map(|s| (s.scope, s.construct))
        .collect()
}

fn one(scope: &str, construct: &str) -> Vec<(String, String)> {
    vec![(scope.to_string(), construct.to_string())]
}

#[test]
fn comments_literals_and_use_declarations_are_blanked() {
    let src = "use std::sync::Mutex;\n// Mutex\n/* a /* nested */ Mutex */\nfn f() { let _ = \"Mutex\"; let _ = r#\"Mutex\"#; let _ = b\"Mutex\"; let _c = '\"'; }\n";
    assert!(scopes(src).is_empty(), "{:?}", scopes(src));
}

#[test]
fn a_lifetime_is_not_a_char_literal() {
    let src = "fn f<'a>(x: &'a str) -> std::sync::Mutex<&'a str> { todo!() }\n";
    assert_eq!(scopes(src), one("f", "lock"));
}

#[test]
fn non_ascii_text_keeps_lines() {
    let src = "// caf\u{e9} \u{1f600}\nfn f() { let _ = '\u{e9}'; std::thread::spawn(|| {}); }\n";
    let units: Vec<u16> = src.encode_utf16().collect();
    let (text, _) = strip_cfg_test(&sanitize(src), &units);
    let s = sites_of("f.rs", &text);
    assert_eq!(s[0].lines, vec![2]);
    assert_eq!(s[0].scope, "f");
}

#[test]
fn scopes_are_named_like_the_ts_census() {
    assert_eq!(
        scopes("impl<T> Foo<T> where T: Copy { fn a(&self) { tokio::spawn(x); } }"),
        one("Foo::a", "tokio-spawn")
    );
    assert_eq!(
        scopes("impl fmt::Display for Bar { fn fmt(&self) { fn inner() { tokio::spawn(x); } } }"),
        one("Bar::fmt::inner", "tokio-spawn")
    );
    assert_eq!(
        scopes("static N: AtomicU64 = AtomicU64::new(0);"),
        one("static N", "atomic")
    );
    assert_eq!(
        scopes("struct S { m: std::sync::Mutex<u8> }"),
        one("struct S", "lock")
    );
    assert_eq!(
        scopes("impl S { const X: AtomicBool = AtomicBool::new(false); }"),
        one("impl S", "atomic")
    );
    assert_eq!(scopes("pub fn sig(m: &Mutex<u8>) {}"), one("sig", "lock"));
    assert_eq!(scopes("type T = Mutex<u8>;"), one("type T", "lock"));
    assert_eq!(
        scopes("mod m { pub fn f() { tokio::spawn(x); } }"),
        one("mod m::f", "tokio-spawn")
    );
    assert_eq!(scopes("x!(Mutex);"), one("<module>", "lock"));
}

#[test]
fn relaxed_is_ordering_not_a_variant() {
    assert_eq!(
        scopes("fn f() { n.load(Relaxed); }"),
        one("f", "atomic-relaxed")
    );
    assert!(scopes("fn f() { Tier::Relaxed; g(Relaxed(1)); }").is_empty());
    assert_eq!(
        scopes("fn f() { tokio::sync::Mutex::new(1); }"),
        one("f", "async-lock")
    );
}

#[test]
fn cfg_test_items_are_blanked_and_modules_collected() {
    let src = "#[cfg(test)]\n#[path = \"x_test.rs\"]\nmod x_test;\n#[cfg(test)]\nmod t { fn f() { tokio::spawn(x); } }\n";
    let units: Vec<u16> = src.encode_utf16().collect();
    let (text, mods) = strip_cfg_test(&sanitize(src), &units);
    assert!(sites_of("f.rs", &text).is_empty());
    assert_eq!(
        mods,
        vec![TestModule {
            name: "x_test".into(),
            path_attr: Some("x_test.rs".into())
        }]
    );
}
