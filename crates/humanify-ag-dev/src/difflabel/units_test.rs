use super::{FreeName, InputFile};

fn unit_texts<'a>(file: &InputFile, text: &'a str) -> Vec<&'a str> {
    file.units
        .iter()
        .map(|u| &text[u.start as usize..u.end as usize])
        .collect()
}

#[test]
fn every_function_and_every_other_top_level_statement_is_a_unit() {
    let text = "import{a}from\"./x.js\";function f(p){return()=>p}var t=f(1);export{t};";
    let file = InputFile::parse("x.js", text);
    assert_eq!(
        unit_texts(&file, text),
        ["function f(p){return()=>p}", "()=>p", "var t=f(1);"]
    );
    assert_eq!(file.units[1].parent, Some(0));
    assert_eq!(file.units[2].parent, None);
}

#[test]
fn renaming_identifiers_keeps_the_text_key_and_changing_code_does_not() {
    let key = |text: &str| InputFile::parse("x.js", text).units[0].text_key;
    let base = key("function f(a,b){let c=a.size+b;return{c}}");
    assert_eq!(base, key("function g(x,y){let z=x.size+y;return{c:z}}"));
    assert_ne!(base, key("function g(x,y){let z=x.size-y;return{c:z}}"));
    assert_ne!(base, key("function g(x,y){let z=x.length+y;return{c:z}}"));
    assert_ne!(base, key("function g(x,y){let z=y.size+x;return{c:z}}"));
}

#[test]
fn the_innermost_unit_holding_an_offset_is_found_through_its_parents() {
    let text = "function f(){var a=()=>1;return a}var q=2;";
    let file = InputFile::parse("x.js", text);
    let at = |needle: &str| file.innermost(text.find(needle).unwrap() as u32);
    assert_eq!(at("()=>1"), Some(1));
    assert_eq!(at("return a"), Some(0));
    assert_eq!(at("var q"), Some(2));
    assert_eq!(
        InputFile::parse("x.js", "export{};var q=2;").innermost(2),
        None
    );
}

#[test]
fn a_unit_lists_the_names_it_reads_from_outside_itself() {
    let text = "var k=1;function f(a){return a+k+window.x+f.length}";
    let file = InputFile::parse("x.js", text);
    let free: Vec<FreeName> = file.free_names(1).into_iter().flatten().collect();
    let k = text.find('k').unwrap() as u32;
    let f = text.find("f(").unwrap() as u32;
    assert_eq!(free.len(), 4);
    assert_eq!(free[0], FreeName::Declared(f));
    assert_eq!(free[1], FreeName::Declared(k));
    assert!(matches!(free[2], FreeName::Global(_)));
    assert_eq!(free[3], FreeName::Declared(f));
}
