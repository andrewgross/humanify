use super::*;

#[test]
fn bun_s_errors_name_what_to_retry() {
    let log = "error: Could not resolve: \"fsevents\"\nerror: Could not resolve \"b\"\nerror: Could not resolve: \"fsevents\"\nNo loader is configured for \".node\" files: x\n  at /a/node_modules/@s/p/lib/x.js and /a/node_modules/q/i.js\n";
    assert_eq!(
        unresolved(log),
        vec!["b".to_string(), "fsevents".to_string()]
    );
    assert_eq!(no_loader(log), vec![".node".to_string()]);
    assert_eq!(
        node_module_pkgs(log),
        vec!["@s/p".to_string(), "q".to_string()]
    );
    // a scope with no package after it is the scope (the optional group backtracks)
    assert_eq!(
        node_module_pkgs("node_modules/@s/x"),
        vec!["@s".to_string()]
    );
}

#[test]
fn json_dump_spellings() {
    let r = J::Obj(vec![
        ("ok".into(), J::Bool(true)),
        ("log".into(), J::Str(String::new())),
    ]);
    assert_eq!(dumps(&r, None), "{\"ok\": true, \"log\": \"\"}");
    let l = J::List(vec![J::Obj(vec![("id".into(), J::Str("a".into()))])]);
    assert_eq!(dumps(&l, Some(1)), "[\n {\n  \"id\": \"a\"\n }\n]");
}

#[test]
fn tail_counts_characters() {
    assert_eq!(tail("abcdé", 2), "dé");
    assert_eq!(tail("ab", 5), "ab");
}
