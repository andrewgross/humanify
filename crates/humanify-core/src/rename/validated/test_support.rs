//! Test helpers: parse a snippet the way the TS fixtures do (Babel
//! `sourceType: "module"` → an ES module; otherwise a sloppy script) and
//! hand the semantic model to a closure (the arena outlives it).

use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_semantic::{Semantic, SemanticBuilder};
use oxc_span::SourceType;

pub(crate) fn with_semantic<R>(code: &str, module: bool, f: impl FnOnce(&Semantic<'_>) -> R) -> R {
    let allocator = Allocator::default();
    let source_type = if module {
        SourceType::mjs()
    } else {
        SourceType::cjs()
    };
    let ret = Parser::new(&allocator, code, source_type).parse();
    assert!(
        ret.diagnostics.is_empty(),
        "fixture must parse clean: {code:?} {:?}",
        ret.diagnostics
    );
    let program = allocator.alloc(ret.program);
    let semantic = SemanticBuilder::new()
        .with_build_nodes(true)
        .build(program)
        .semantic;
    f(&semantic)
}
