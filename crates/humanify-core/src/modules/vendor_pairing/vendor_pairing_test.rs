use super::*;

fn idf_of(prior: &[ModuleFeatures]) -> HashMap<String, f64> {
    literal_idf(prior.iter())
}

#[test]
fn the_lexer_masks_minified_names_and_keeps_strings_regexes_and_numbers() {
    let toks =
        lex("var aB=function(q){return /a\\/b[/]c/g.test(q)?0x1F:1.5e3}// gone\n/* gone */`t${x}`");
    assert_eq!(
        toks,
        vec![
            "var",
            "I",
            "=",
            "function",
            "(",
            "I",
            ")",
            "{",
            "return",
            "/a\\/b[/]c/g",
            ".",
            "I",
            "(",
            "I",
            ")",
            "?",
            "0x1F",
            ":",
            "1.5e3",
            "}",
            "`t${x}`"
        ]
    );
    // lib_<hash8>[_n] is the unpack's own minted name: masked too.
    assert_eq!(
        lex("lib_0123abcd lib_0123abcd_2 library"),
        vec!["I", "I", "library"]
    );
    // After an identifier a `/` divides.
    assert_eq!(lex("a/b/c"), vec!["I", "/", "I", "/", "I"]);
}

#[test]
fn a_long_string_is_split_into_words_so_a_prose_edit_costs_only_its_words() {
    let prose = |w: &str| {
        module_features(&format!(
            "function f(){{return \"This is a long prompt about {w} with many more words after it\"}}"
        ))
    };
    let (a, b) = (prose("cats"), prose("kittens"));
    let shared = a.shingles.iter().filter(|s| b.shingles.contains(s)).count();
    assert!(
        shared * 2 > a.shingles.len(),
        "most runs survive a one-word edit"
    );
}

#[test]
fn literals_are_strings_and_longer_identifiers() {
    let f = module_features("function f(){return x.someProperty+\"value!\"+\"ab\"}");
    assert_eq!(
        f.literals,
        vec!["#function", "#return", "#someProperty", "value!"]
    );
}

#[test]
fn a_clear_twin_pairs_and_a_tie_does_not() {
    let module = |s: &str| {
        module_features(&format!(
            "function(e,t){{e.exports=function parse(input){{if(!input)throw new Error(\"{s}\");return input.split(\",\").map(function(v){{return v.trim()}}).filter(Boolean)}}}}"
        ))
    };
    let unrelated =
        module_features("function(e){e.render=function(n){return \"<div>\"+n+\"</div>\"}}");
    // One fresh module, its edited twin and an unrelated prior: paired.
    let prior = vec![unrelated.clone(), module("missing input value")];
    let fresh = vec![module("missing input value!!")];
    let pairs = pair_by_content(&fresh, &prior, &idf_of(&prior));
    assert_eq!(pairs.len(), 1);
    assert_eq!((pairs[0].fresh, pairs[0].prior), (0, 1));
    assert!(pairs[0].score >= MIN_SCORE && pairs[0].fresh_margin >= MIN_MARGIN);
    // Two equally good priors: no margin, no pair.
    let prior = vec![module("missing input one"), module("missing input two")];
    let fresh = vec![module("missing input six")];
    assert!(pair_by_content(&fresh, &prior, &idf_of(&prior)).is_empty());
    // Two fresh modules after one prior: the prior's side has no margin.
    let prior = vec![module("missing input value")];
    let fresh = vec![module("missing input one"), module("missing input two")];
    assert!(pair_by_content(&fresh, &prior, &idf_of(&prior)).is_empty());
}

#[test]
fn nothing_alike_never_pairs() {
    let prior = vec![module_features("function(e){e.a=1}")];
    let fresh = vec![module_features(
        "function(e){e.render=function(n){return \"<section>\"+n+\"</section>\"}}",
    )];
    assert!(pair_by_content(&fresh, &prior, &idf_of(&prior)).is_empty());
}

#[test]
fn a_rare_shared_literal_outweighs_common_ones() {
    // The literal score: rare-weighted, so two modules sharing only common
    // literals score low while a shared rare one counts.
    let common: Vec<ModuleFeatures> = (0..20)
        .map(|i| module_features(&format!("function(e){{e.x{i}=\"commonvalue\"}}")))
        .collect();
    let idf = idf_of(&common);
    assert!(
        idf["commonvalue"]
            < idf
                .get("#somethingRare")
                .copied()
                .unwrap_or(UNSEEN_LITERAL_IDF)
    );
}

#[test]
fn only_a_safe_vendor_path_is_reused() {
    assert!(reusable_vendor_path("vendor/js-yaml.js", "vendor"));
    assert!(reusable_vendor_path(
        "vendor/@aws-sdk/lib_0123abcd-2.js",
        "vendor"
    ));
    assert!(!reusable_vendor_path("vendor/../src/x.js", "vendor"));
    assert!(!reusable_vendor_path(
        "src/_assets/environment.js",
        "vendor"
    ));
    assert!(!reusable_vendor_path("vendor/x.json", "vendor"));
    assert!(!reusable_vendor_path("vendor/a b.js", "vendor"));
}
