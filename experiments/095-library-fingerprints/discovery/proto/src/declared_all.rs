//! exp095 index-v2 lane: the pipeline's own declared readers
//! (`identity::scan_app_declarations`) over EVERY top-level statement of a
//! raw bundle (vendor factories included), as JSON lines
//! {reader, package, version, client, start, end}. What the readers name is
//! what an index built for this bundle should pin.
//!
//! Usage: exp095-declared-all <bundle.js> > declared.jsonl
use humanify_core::ingest::Ingest;
use humanify_core::toolchain::BundleLayout;

fn main() {
    let path = std::env::args().nth(1).expect("bundle");
    let code = std::fs::read_to_string(&path).expect("bundle");
    let allocator = oxc_allocator::Allocator::default();
    let ingest = Ingest::parse_unambiguous(&allocator, &code);
    assert!(ingest.errors.is_empty(), "parse errors");
    let wrapper =
        BundleLayout::SingleWrapperFunction.find_wrapper(ingest.program, ingest.semantic());
    let statements = humanify_core::modules::container_statements(
        ingest.program,
        ingest.semantic(),
        wrapper.as_ref().map(|w| w.body_span),
    );
    use oxc_span::GetSpan;
    let spans: Vec<oxc_span::Span> = statements.iter().map(|s| s.span()).collect();
    for d in humanify_core::modules::identity::scan_app_declarations(&code, &spans) {
        println!(
            "{}",
            serde_json::json!({"reader": d.reader, "package": d.package, "version": d.version,
                "client": d.client, "start": d.start, "end": d.end})
        );
    }
}
