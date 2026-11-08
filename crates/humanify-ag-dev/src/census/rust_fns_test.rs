//! The censuses' one view of Rust source: which functions it sees, what
//! kind each is, its CC (rule `slop/1`), its SLOC and its masked stream.

use std::path::PathBuf;

use super::*;

/// Plant `files` (repo-relative path, text) under a fresh temp directory.
pub(crate) fn plant(tag: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("census-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    for (rel, text) in files {
        let p = dir.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }
    dir
}

fn units_at(rel: &str, src: &str) -> Vec<FnUnit> {
    parse_source(rel, src).expect("parses").units
}

fn units(src: &str) -> Vec<FnUnit> {
    units_at("crates/k/src/a.rs", src)
}

fn by_name<'a>(us: &'a [FnUnit], name: &str) -> &'a FnUnit {
    us.iter().find(|u| u.name == name).unwrap_or_else(|| {
        panic!(
            "no unit {name}: {:?}",
            us.iter().map(|u| &u.name).collect::<Vec<_>>()
        )
    })
}

const CC_SRC: &str = r#"
fn plain() -> u32 { 1 }

fn every(a: bool, b: bool, xs: &[u32]) -> Result<u32, String> {
    let n: u32 = "1".parse().map_err(|_| String::new())?;
    if a {
    } else if b {
    } else {
    }
    while a {
        break;
    }
    for _x in xs {}
    loop {
        break;
    }
    let _c = a && b || a;
    match n {
        0 => {}
        1 => {}
        _ => {}
    }
    let f = |y: u32| if y > 0 { y } else { 0 };
    Ok(f(n))
}

fn lets(o: Option<u32>) -> u32 {
    if let Some(v) = o { v } else { 0 }
}

fn questions(s: &str) -> Result<u32, std::num::ParseIntError> {
    let a = s.parse::<u32>()?;
    let b = s.parse::<u32>()?;
    Ok(a + b)
}
"#;

#[test]
fn cc_counts_every_construct_of_slop_1_and_not_the_question_mark() {
    let us = units(CC_SRC);
    assert_eq!(by_name(&us, "plain").cc, 1);
    // 1 + if/else-if 2 + while 1 + for 1 + loop 1 + && || 2 + match arms
    // after the first 2 + the closure's if 1 = 11; the `?` adds nothing.
    assert_eq!(by_name(&us, "every").cc, 11);
    assert_eq!(by_name(&us, "lets").cc, 2, "`if let` is an if");
    assert_eq!(by_name(&us, "questions").cc, 1, "`?` is not counted");
}

#[test]
fn sloc_skips_blank_and_comment_lines() {
    let us = units("fn f() {\n    // a comment\n\n    let a = 1;\n    let _b = a;\n}\n");
    assert_eq!(us[0].sloc, 4, "fn line, two statements, closing brace");
    assert_eq!(us[0].line, 1);
}

#[test]
fn names_and_kinds_follow_the_items() {
    let src = r#"
struct Foo;
impl Foo {
    fn bar(&self) {}
}
trait Tr {
    fn m(&self) { fn nested() {} }
    fn required(&self);
}
#[test]
fn a_test() {}
#[cfg(test)]
mod tests {
    fn helper() {}
}
mod inner {
    pub fn f() {}
}
"#;
    let us = units(src);
    let names: Vec<(&str, Kind)> = us.iter().map(|u| (u.name.as_str(), u.kind)).collect();
    assert_eq!(
        names,
        [
            ("Foo::bar", Kind::NonTest),
            ("Tr::m", Kind::NonTest),
            ("a_test", Kind::Test),
            ("tests::helper", Kind::Test),
            ("inner::f", Kind::NonTest),
        ],
        "nested fns belong to their parent; bodiless trait fns are no unit"
    );
    for rel in ["crates/k/src/a_test.rs", "crates/k/tests/it.rs"] {
        let us = units_at(rel, "fn g() {}");
        assert_eq!(us[0].kind, Kind::Test, "{rel}");
    }
    assert_eq!(units("fn g() {}")[0].krate, "k");
}

#[test]
fn cognitive_complexity_allows_are_listed_by_name() {
    let src = r#"
struct Foo;
impl Foo {
    #[allow(clippy::cognitive_complexity)]
    fn big(&self) {}
    #[allow(dead_code)]
    fn other(&self) {}
}
"#;
    let p = parse_source("crates/k/src/a.rs", src).unwrap();
    let sites: Vec<(&str, usize)> = p.allows.iter().map(|a| (a.name.as_str(), a.line)).collect();
    assert_eq!(sites, [("Foo::big", 4)]);
}

#[test]
fn masking_hides_bindings_and_keeps_everything_else() {
    let src = r#"
fn one(items: &[u32]) -> u32 { let total = items.iter().sum::<u32>(); total + "abc".len() as u32 }
fn two(xs: &[u32]) -> u32 { let acc = xs.iter().sum::<u32>(); acc + "abc".len() as u32 }
fn method(xs: &[u32]) -> u32 { let acc = xs.iter().product::<u32>(); acc + "abc".len() as u32 }
fn literal(xs: &[u32]) -> u32 { let acc = xs.iter().sum::<u32>(); acc + "xyz".len() as u32 }
"#;
    let us = units(src);
    let (one, two) = (by_name(&us, "one"), by_name(&us, "two"));
    assert_eq!(
        one.exact, two.exact,
        "renamed bindings (and fn name) mask alike"
    );
    assert_ne!(
        one.exact,
        by_name(&us, "method").exact,
        "a method name stays verbatim"
    );
    let lit = by_name(&us, "literal");
    assert_ne!(
        one.exact, lit.exact,
        "literals are exact in the strict stream"
    );
    assert_eq!(one.blurred, lit.blurred, "same-length literals blur alike");
    assert!(one.tokens > 20, "{}", one.tokens);
}

#[test]
fn worktree_sources_walk_crates_and_skip_target() {
    let root = plant(
        "walk",
        &[
            ("crates/k/src/lib.rs", "fn a() {}"),
            ("crates/k/target/debug/x.rs", "fn b() {}"),
            ("other/c.rs", "fn c() {}"),
        ],
    );
    let rels: Vec<String> = worktree_sources(&root)
        .unwrap()
        .into_iter()
        .map(|s| s.rel)
        .collect();
    assert_eq!(rels, ["crates/k/src/lib.rs"]);
}
