//! The immutable-literal extractor: what a minifier cannot rename, read the
//! same way from an npm source file and from the bundled module it becomes
//! (exp095 immutable-literals: 95% of lab spans keep 99-100% of their
//! source literals under every Bun version and esbuild).

use std::collections::BTreeSet;

use super::literals::{LiteralMode, code_literals, json_literals};

fn lits(code: &str, mode: LiteralMode) -> BTreeSet<String> {
    code_literals(code, mode).expect("parses")
}

fn set(items: &[&str]) -> BTreeSet<String> {
    items.iter().map(|s| (*s).to_string()).collect()
}

#[test]
fn strings_properties_numbers_and_regexes_are_read_by_kind() {
    let got = lits(
        r#"var o = { long: 1000, "short": f }; o.parse(/^\d+ms$/i, "val is not", 31556952, 2n);"#,
        LiteralMode::Bundled,
    );
    assert_eq!(
        got,
        set(&[
            "N:1000",
            "N:2n",
            "N:31556952",
            "P:long",
            "P:parse",
            "P:short",
            r"R:/^\d+ms$/i",
            "S:val is not",
        ])
    );
}

#[test]
fn zero_one_and_directives_say_nothing_about_a_package() {
    let got = lits(
        r#""use strict"; var a = 0, b = 1, c = 2;"#,
        LiteralMode::Bundled,
    );
    assert_eq!(got, set(&["N:2"]));
}

#[test]
fn template_chunks_and_folded_concatenations_are_strings() {
    let got = lits(
        r#"f(`count: ${n} items`, "ab" + "cd");"#,
        LiteralMode::Bundled,
    );
    assert_eq!(
        got,
        set(&["S: items", "S:ab", "S:abcd", "S:cd", "S:count: "])
    );
}

#[test]
fn a_computed_constant_key_is_a_property_not_a_string() {
    let got = lits(
        r#"x["content-type"] = { ["x-id"]: 3 };"#,
        LiteralMode::Bundled,
    );
    assert_eq!(got, set(&["N:3", "P:content-type", "P:x-id"]));
}

#[test]
fn undefined_and_the_minifiers_u_read_as_one_literal() {
    // `typeof x == "undefined"` is `typeof x > "u"` after minification.
    let source = lits(r#"if (typeof x == "undefined") y();"#, LiteralMode::Source);
    let bundled = lits(r#"if (typeof x > "u") y();"#, LiteralMode::Bundled);
    assert_eq!(source, set(&["S:u"]));
    assert_eq!(source, bundled);
}

#[test]
fn builtin_module_names_take_one_spelling() {
    let a = lits(r#"require("fs"); g("node:path");"#, LiteralMode::Source);
    assert_eq!(a, set(&["S:node:fs", "S:node:path"]));
}

#[test]
fn a_long_value_is_kept_as_a_hash_prefix() {
    let long = "x".repeat(121);
    let got = lits(&format!(r#"f("{long}");"#), LiteralMode::Bundled);
    let only = got.iter().next().expect("one literal");
    assert_eq!(got.len(), 1);
    assert!(only.starts_with("S#"), "{only}");
    assert_eq!(only.len(), 2 + 16);
    let at_limit = "y".repeat(120);
    let kept = lits(&format!(r#"f("{at_limit}");"#), LiteralMode::Bundled);
    assert_eq!(kept, set(&[&format!("S:{at_limit}")]));
}

#[test]
fn source_mode_drops_what_bundling_removes_and_adds_what_it_adds() {
    // Bundling turns a bundled require/import into a factory call (the
    // specifier is gone) and prints ESM export names as keys.
    let got = lits(
        r#"import { parse as p } from "./parse.js";
import def from "lodash";
export const VERSION = "1.2.3";
export function format(v) { return p(v); }
export { p as reparse };
export * as utils from "./utils.js";
export default 42;
const x = require("./helper");
import("./lazy.js");"#,
        LiteralMode::Source,
    );
    assert_eq!(
        got,
        set(&[
            "N:42",
            "P:VERSION",
            "P:default",
            "P:format",
            "P:parse",
            "P:reparse",
            "P:utils",
            "S:1.2.3",
        ])
    );
}

#[test]
fn bundled_mode_keeps_every_string_a_bundle_still_holds() {
    // In a bundle, a surviving `require("x")` is an EXTERNAL the bundle
    // really names.
    let got = lits(r#"const x = require("./helper");"#, LiteralMode::Bundled);
    assert_eq!(got, set(&["S:./helper"]));
}

#[test]
fn a_source_file_and_its_minified_build_share_their_literals() {
    let source = lits(
        r#"'use strict';
const ms = require('ms');
module.exports = function humanize(val, options) {
  options = options || {};
  if (typeof val === 'string' && val.length > 0) return ms(val);
  throw new Error('val is not a non-empty string or a valid number. val=' + JSON.stringify(val));
};"#,
        LiteralMode::Source,
    );
    let bundled = lits(
        r#"var B=m((ge,q)=>{var h=c();q.exports=function(e,r){r=r||{};if(typeof e==="string"&&e.length>0)return h(e);throw Error("val is not a non-empty string or a valid number. val="+JSON.stringify(e))}});"#,
        LiteralMode::Bundled,
    );
    assert!(source.len() >= 3, "{source:?}");
    let shared: BTreeSet<&String> = source.intersection(&bundled).collect();
    assert_eq!(source.len(), shared.len(), "{source:?} vs {bundled:?}");
}

#[test]
fn text_that_does_not_parse_has_no_literal_set() {
    assert_eq!(code_literals("function (", LiteralMode::Source), None);
}

#[test]
fn json_keys_are_properties_and_values_are_strings_and_numbers() {
    let got = json_literals(r#"{"name":"ms","version":"2.1.3","files":["index.js"],"n":7}"#)
        .expect("json");
    assert_eq!(
        got,
        set(&[
            "N:7",
            "P:files",
            "P:n",
            "P:name",
            "P:version",
            "S:2.1.3",
            "S:index.js",
            "S:ms",
        ])
    );
    assert_eq!(json_literals("{nope"), None);
}
