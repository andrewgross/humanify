//! The inline module recognisers (exp105): what is a module body and what
//! only looks like one.

use oxc_allocator::Allocator;
use oxc_ast::ast::Statement;

use crate::ingest::Ingest;
use crate::modules::inline_modules::{
    rollup_declarator_modules, rollup_statement_modules, webpack_declarator_modules,
    webpack_statement_scopes,
};

/// (statement modules, declarator modules) of `code`'s statements: per
/// module, the first 10 chars of its wrapper and how many statements its
/// body has.
fn found(code: &str, webpack: bool) -> Vec<(String, usize)> {
    let allocator = Allocator::default();
    let ingest = Ingest::parse(&allocator, code, "input.js");
    assert!(ingest.errors.is_empty(), "{code}");
    let mut out = Vec::new();
    for stmt in &ingest.program.body {
        let mut modules = if webpack {
            webpack_statement_scopes(stmt)
        } else {
            rollup_statement_modules(stmt)
        };
        if let Statement::VariableDeclaration(v) = stmt {
            for d in &v.declarations {
                modules.extend(if webpack {
                    webpack_declarator_modules(d)
                } else {
                    rollup_declarator_modules(d)
                });
            }
        }
        out.extend(modules.into_iter().map(|(span, body)| {
            (
                code[span.start as usize..span.end as usize]
                    .chars()
                    .take(10)
                    .collect(),
                body.len(),
            )
        }));
    }
    out
}

fn rows(list: &[(&str, usize)]) -> Vec<(String, usize)> {
    list.iter().map(|(t, n)| (t.to_string(), *n)).collect()
}

#[test]
fn a_webpack_module_table_is_every_numeric_entry_or_nothing() {
    let code = "var e={12(e,t){t.a=1;t.b=2},34:(e)=>{e.exports=3},56:function(e,t,n){var r=n(12);e.exports=r}};";
    assert_eq!(
        found(code, true),
        rows(&[("12(e,t){t.", 2), ("34:(e)=>{e", 1), ("56:functio", 2)])
    );
    // a string key, a non-function value, an expression-bodied arrow: an
    // object of the app's own, not a table
    for not_a_table in [
        "var e={12(e){e.x=1},a(e){e.y=2}};",
        "var e={12(e){e.x=1},34:5};",
        "var e={12:e=>e.exports};",
    ] {
        assert_eq!(found(not_a_table, true), Vec::new(), "{not_a_table}");
    }
}

#[test]
fn webpack_s_commonjs_wrapper_and_entry_iifes_are_modules() {
    let code = "var e=r.cw(function(e,s){var x=1;e.exports=x});(()=>{\"use strict\";var a=1;console.log(a)})(),module.exports=t;!function(){var b=2}();";
    assert_eq!(
        found(code, true),
        rows(&[("function(e", 2), ("()=>{\"use ", 2), ("function()", 1)])
    );
    // another member call is not the wrapper
    assert_eq!(found("var e=r.n(function(e){e.x=1});", true), Vec::new());
}

#[test]
fn rollup_s_guarded_require_functions_are_modules() {
    let code = "function requireMs(){if(h)return m;h=1;var e=1;return m=e}var a=function(){if(t)return r;t=1,r=2;return r}(),c=g(function(){if(n)return s;n=1;return s=3}());";
    assert_eq!(
        found(code, false),
        rows(&[("function r", 4), ("function()", 3), ("function()", 3)])
    );
    for not_guarded in [
        // the flag is set to something else
        "function f(){if(h)return m;h=2;return m}",
        // the flag set is another binding
        "function f(){if(h)return m;k=1;return m}",
        // parameters: an ordinary memoised function
        "function f(x){if(h)return m;h=1;return m=x}",
        // called with an argument
        "var a=function(){if(t)return r;t=1;return r}(1);",
    ] {
        assert_eq!(found(not_guarded, false), Vec::new(), "{not_guarded}");
    }
}
