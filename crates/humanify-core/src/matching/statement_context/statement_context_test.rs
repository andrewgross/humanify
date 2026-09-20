//! Tests for the enclosing-statement contexts (`statement_context.rs`) — the
//! statement walk's babel-Statement alias and the per-row contexts.

use oxc_allocator::Allocator;

use crate::graph::{GraphFunction, UnifiedGraph, build_unified_graph};
use crate::hash::serialize::SymbolTables;
use crate::ingest::Ingest;
use crate::matching::statement_context::{StatementContexts, StmtUsability};

fn contexts_of(code: &str) -> (String, &'static UnifiedGraph, StatementContexts) {
    let text: &'static str = Box::leak(code.to_string().into_boxed_str());
    let allocator: &'static Allocator = Box::leak(Box::new(Allocator::default()));
    let ingest = Ingest::parse(allocator, text, "input.js");
    assert!(ingest.errors.is_empty(), "must parse: {:?}", ingest.errors);
    let tables = SymbolTables::build(&ingest.semantic);
    let graph: &'static UnifiedGraph = Box::leak(Box::new(build_unified_graph(
        &ingest.semantic,
        ingest.program,
        "input.js",
        &[],
        None,
        None,
    )));
    let ctx = StatementContexts::build(graph, &ingest.semantic, &tables, ingest.program, text);
    (text.to_string(), graph, ctx)
}

/// The fn row whose source text contains `needle`, picking the innermost
/// (an enclosing row's text contains an inner row's text too).
fn fn_row<'g>(graph: &'g UnifiedGraph, text: &str, needle: &str) -> &'g GraphFunction {
    graph
        .functions
        .iter()
        .filter(|f| text[f.span.start as usize..f.span.end as usize].contains(needle))
        .min_by_key(|f| f.span.end - f.span.start)
        .unwrap_or_else(|| panic!("no function row containing {needle:?}"))
}

#[test]
fn loop_head_statements_are_the_enclosing_statement() {
    // babel's Statement alias includes the LOOP forms (ForOfStatement,
    // ForInStatement, ForStatement, WhileStatement, DoWhileStatement —
    // probed against @babel/types): TS `getStatementParent` stops at the
    // loop HEAD when the function sits inside it, NOT at the loop's
    // enclosing block. Missing them made the walk climb past the loop to
    // the block — a statement over the 50-line cap that TS hashed fine
    // (the WP2.1 pair 2.1.85→86: a `for await (… of agentGenerator({
    // onCacheSafeParams: cond ? arrow : undefined }))` head).
    let (text, graph, ctx) = contexts_of(
        r#"
        async function main() {
            for await (let x of gen({ cb: () => { return 1; } })) { work(x); }
            for (let i = init(() => { return 2; }); i < 2; i++) { work(i); }
            for (const k of gen({ cb: () => { return 3; } })) { work(k); }
            for (const k in gen({ cb: () => { return 4; } })) { work(k); }
            while (check(() => { return 5; })) { work(); }
            do { work(); } while (check(() => { return 6; }));
            if (check(() => { return 7; })) { work(); }
            while (other()) { tap(() => { return 8; }); }
        }
    "#,
    );
    // Every arrow but the last sits in a loop/if HEAD — babel's
    // getStatementParent stops at the loop/if statement itself (the
    // classic-for case stops at its INIT declaration — a VariableDeclaration
    // is a Statement wherever it appears). The last one sits in a loop
    // BODY — its statement is the inner `tap(...)` call.
    let cases: [(&str, &str); 8] = [
        ("return 1;", "for await"),
        ("return 2;", "let i = init"),
        ("return 3;", "for (const k of"),
        ("return 4;", "for (const k in"),
        ("return 5;", "while (check"),
        ("return 6;", "do {"),
        ("return 7;", "if (check"),
        ("return 8;", "tap(()"),
    ];
    for (needle, head) in cases {
        let f = fn_row(graph, &text, needle);
        let index = graph
            .functions
            .iter()
            .position(|g| g.span == f.span)
            .unwrap();
        let row = ctx.function_rows().get(index).expect("row context");
        let stmt = row.stmt_span.expect("a statement");
        let head_off = text
            .find(head)
            .unwrap_or_else(|| panic!("{head} not in text")) as u32;
        assert_eq!(
            stmt.start, head_off,
            "{needle}: the enclosing statement must be the {head} head, not the enclosing block"
        );
        assert!(
            matches!(row.usability, StmtUsability::Ok { .. }),
            "{needle}: the loop head is under the cap (the block is not)"
        );
    }
}
