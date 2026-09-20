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
    // getStatementParent stops at the loop/if statement itself. The
    // classic-for case stops at the ForStatement too, NOT at its INIT
    // declaration: the init VariableDeclaration is a Statement, but it sits
    // in the for's `init` field — a NON-array container — and babel's break
    // requires `Array.isArray(path.container)` (probed against
    // @babel/traverse, 2026-09-20). The last one sits in a loop BODY — its
    // statement is the inner `tap(...)` call.
    let cases: [(&str, &str); 8] = [
        ("return 1;", "for await"),
        ("return 2;", "for (let i = init"),
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

/// babel's break needs the statement to sit in an ARRAY container
/// (`Array.isArray(path.container) && path.isStatement()`,
/// @babel/traverse ancestry.js:37). A Statement in a single-statement slot
/// (an unbraced `if`/`for` body, an `else if` chain's nested arm) or in an
/// export's `declaration` field is NOT the answer — the climb continues.
/// Probed against @babel/traverse, 2026-09-20; the miss was the LAST WP2.1
/// matches-gate divergence (2.1.85→86: 28 prior-side rows resolved their
/// statement one slot short, 7 of them crossing a span bucket).
#[test]
fn statements_in_non_array_containers_are_climbed_past() {
    let (text, graph, ctx) = contexts_of(
        r#"
        function toplevel() { return 1; }
        function main() {
            if (cond) assigned = () => { return 2; };
            for (const k of list) table[k] = () => { return 3; };
            if (a) first();
            else if (b) chained = () => { return 4; };
            while (go) stepped = () => { return 5; };
            {
                blocked = () => { return 6; };
            }
        }
    "#,
    );
    // (needle, the statement the climb must land on — by its source prefix)
    let cases: [(&str, &str); 6] = [
        // A top-level FunctionDeclaration sits in Program.body — an array —
        // so it IS its own statement.
        ("return 1;", "function toplevel"),
        ("return 2;", "if (cond)"),
        ("return 3;", "for (const k of list)"),
        ("return 4;", "if (a) first()"),
        ("return 5;", "while (go)"),
        // A braced block body IS an array container — the inner
        // ExpressionStatement is the answer there.
        ("return 6;", "blocked = () =>"),
    ];
    for (needle, prefix) in cases {
        let f = fn_row(graph, &text, needle);
        let index = graph
            .functions
            .iter()
            .position(|g| g.span == f.span)
            .unwrap();
        let row = ctx.function_rows().get(index).expect("row context");
        let stmt = row.stmt_span.expect("a statement");
        let want = text
            .find(prefix)
            .unwrap_or_else(|| panic!("{prefix} not in text")) as u32;
        assert_eq!(
            stmt.start, want,
            "{needle}: the enclosing statement must be the {prefix:?} statement"
        );
    }
    // A top-level declaration in a list IS its own statement; an arrow in
    // an expression statement is not.
    let toplevel = fn_row(graph, &text, "return 1;");
    let toplevel_row = ctx
        .function_rows()
        .get(
            graph
                .functions
                .iter()
                .position(|g| g.span == toplevel.span)
                .unwrap(),
        )
        .expect("row context");
    assert!(
        toplevel_row.is_own_statement,
        "a top-level function declaration is its own statement"
    );
    let blocked = fn_row(graph, &text, "return 6;");
    let arrow_row = ctx
        .function_rows()
        .get(
            graph
                .functions
                .iter()
                .position(|g| g.span == blocked.span)
                .unwrap(),
        )
        .expect("row context");
    assert!(
        !arrow_row.is_own_statement,
        "an expression-statement arrow is not its own statement"
    );
}
