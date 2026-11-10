//! Source confirmation (exp213): a unit row is kept only when its source
//! file holds its strings and property names.

use std::collections::BTreeSet;

use super::confirm::{SourceWords, Verdict, found_share, verdict};

fn tokens(list: &[&str]) -> BTreeSet<String> {
    list.iter().map(|t| t.to_string()).collect()
}

/// readable-stream 3.6.2's `lib/internal/streams/async_iterator.js`, cut
/// down: what Bun's map named as the source of string_decoder's code.
const ASYNC_ITERATOR: &str = r#"'use strict';
var _Object$setPrototypeO;
var finished = require('./end-of-stream');
var kLastResolve = Symbol('lastResolve');
var kLastReject = Symbol('lastReject');
var kError = Symbol('error');
var kEnded = Symbol('ended');
function createIterResult(value, done) {
  return { value: value, done: done };
}
function readAndResolve(iter) {
  var resolve = iter[kLastResolve];
  if (resolve !== null) {
    var data = iter[kStream].read();
    if (data !== null) {
      iter[kLastPromise] = null;
      resolve(createIterResult(data, false));
    }
  }
}
"#;

/// string_decoder 1.3.0's `lib/string_decoder.js`, cut down.
const STRING_DECODER: &str = r#"'use strict';
var Buffer = require('safe-buffer').Buffer;
var isEncoding = Buffer.isEncoding || function (encoding) {
  encoding = '' + encoding;
  switch (encoding && encoding.toLowerCase()) {
    case 'hex':case 'utf8':case 'utf-8':case 'ascii':case 'binary':case 'base64':case 'ucs2':case 'ucs-2':case 'utf16le':case 'utf-16le':case 'raw':
      return true;
    default:
      return false;
  }
};
"#;

/// The minified `isEncoding` statement's tokens, as the unit print reads
/// them.
fn is_encoding() -> BTreeSet<String> {
    tokens(&[
        "P:isEncoding",
        "P:toLowerCase",
        "S:hex",
        "S:utf8",
        "S:utf-8",
        "S:ascii",
        "S:binary",
        "S:base64",
        "S:ucs2",
        "S:ucs-2",
        "S:utf16le",
        "S:utf-16le",
        "S:raw",
        "G:Buffer",
    ])
}

#[test]
fn a_unit_whose_source_file_holds_its_tokens_is_confirmed() {
    let own = SourceWords::of(STRING_DECODER);
    assert_eq!(verdict(&is_encoding(), &[Some(&own)]), Verdict::Confirmed);
    let (share, n) = found_share(&is_encoding(), &[&own]);
    assert_eq!((share, n), (1.0, 13), "free identifiers are not checked");
}

#[test]
fn a_unit_the_map_filed_under_a_neighbour_is_refused() {
    let neighbour = SourceWords::of(ASYNC_ITERATOR);
    assert_eq!(
        verdict(&is_encoding(), &[Some(&neighbour)]),
        Verdict::Refused
    );
}

#[test]
fn a_unit_spanning_two_files_of_its_package_reads_both() {
    let a = SourceWords::of(ASYNC_ITERATOR);
    let b = SourceWords::of(STRING_DECODER);
    assert_eq!(
        verdict(&is_encoding(), &[Some(&a), Some(&b)]),
        Verdict::Confirmed
    );
}

#[test]
fn a_unit_with_too_few_checkable_tokens_keeps_its_row() {
    let neighbour = SourceWords::of(ASYNC_ITERATOR);
    // short words say nothing; regexes and globals are not read
    let few = tokens(&[
        "S:a",
        "P:id",
        "R:^x+$",
        "G:Buffer",
        "S:utf16le",
        "P:isEncoding",
    ]);
    assert_eq!(verdict(&few, &[Some(&neighbour)]), Verdict::NoTokens);
    assert_eq!(
        verdict(&tokens(&[]), &[Some(&neighbour)]),
        Verdict::NoTokens
    );
}

#[test]
fn a_unit_whose_source_cannot_be_read_keeps_its_row() {
    let own = SourceWords::of(STRING_DECODER);
    assert_eq!(
        verdict(&is_encoding(), &[Some(&own), None]),
        Verdict::Unreadable
    );
}

#[test]
fn a_string_is_found_word_by_word_and_a_minority_miss_is_tolerated() {
    // "Unknown encoding: " is found when both of its words are; the
    // minifier's folded "production" (a --define) is a miss, outvoted
    let src = SourceWords::of(
        "if (typeof nenc !== 'string') throw new Error('Unknown ' + 'encoding: ' + enc); this.lastNeed = 0; this.lastTotal = 0;",
    );
    let unit = tokens(&[
        "S:Unknown encoding: ",
        "S:production",
        "P:lastNeed",
        "P:lastTotal",
        "S:string",
    ]);
    assert_eq!(found_share(&unit, &[&src]), (0.8, 5));
    assert_eq!(verdict(&unit, &[Some(&src)]), Verdict::Confirmed);
}

/// A source spells a control character as an escape (`'\0SLASH'`,
/// `"\x00__link"`, `"\nfoo"`); the unit's token is the cooked string. Its
/// words are found behind the escape (exp213 sample: brace-expansion's
/// `\0SLASH` sentinels, echarts' `\0__throttle…` keys were refused).
#[test]
fn a_string_the_source_spells_with_escapes_is_found() {
    let src = SourceWords::of(
        "var escSlash = '\\0SLASH'+Math.random()+'\\0';\nvar escOpen = '\\0OPEN'+Math.random()+'\\0';\nvar a = \"\\x00__throttleRate\", b = \"\\u0000__link_datas\", c = '\\nCLOSE';",
    );
    let unit = tokens(&[
        "S:\0SLASH",
        "S:\0OPEN",
        "S:\0__throttleRate",
        "S:\0__link_datas",
        "S:\nCLOSE",
        "P:random",
    ]);
    assert_eq!(found_share(&unit, &[&src]), (1.0, 6));
}

/// A bundler that inlines `__dirname` / `__filename` writes the BUILD
/// host's absolute path as a string: no source file spells it, so it is
/// not checkable (exp213 sample: import-fresh, terser-webpack-plugin).
#[test]
fn a_build_host_path_the_bundler_inlined_is_not_checked() {
    let src = SourceWords::of(
        "const path = require('path');\nmodule.exports = (moduleId) => path.resolve(__dirname, moduleId);",
    );
    let unit = tokens(&[
        "S:/work/exp152-data/work/x@1.0.0/node_modules/import-fresh",
        "S:/work/exp152-data/work/x@1.0.0/node_modules/import-fresh/index.js",
        "S:path",
        "P:resolve",
        "S:moduleId",
    ]);
    assert_eq!(found_share(&unit, &[&src]), (1.0, 3));
}
