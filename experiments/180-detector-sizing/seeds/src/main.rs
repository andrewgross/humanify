//! exp180: the text-based package discovery `--library-index-from-bundle`
//! seeds its build from (crates/humanify-cli/src/runtime_index.rs
//! `bundle_revealed_seeds`, replicated line for line because it is
//! crate-private): banner comments, then the app-declaration scan (inlined
//! package.json objects and declared readers). One JSON line per bundle:
//! {"bundle": <path>, "seeds": [{"name", "version", "evidence"}]}.
use std::collections::BTreeMap;

fn seeds(code: &str) -> Vec<(String, Option<String>, String)> {
    let mut by_name: BTreeMap<String, (String, Option<String>, String)> = BTreeMap::new();
    let mut admit =
        |name: String, version: Option<String>, evidence: String| match by_name.get(&name) {
            Some(held) if held.1.is_some() || version.is_none() => {}
            _ => {
                by_name.insert(name.clone(), (name, version, evidence));
            }
        };
    for region in humanify_core::libdetect::find_comment_regions(code) {
        if region.library_name.is_empty() {
            continue;
        }
        admit(
            region.library_name,
            None,
            format!("banner comment at byte {}", region.start),
        );
    }
    let allocator = oxc_allocator::Allocator::default();
    let ingest = humanify_core::ingest::Ingest::parse_unambiguous(&allocator, code);
    if ingest.errors.is_empty() {
        use oxc_span::GetSpan;
        let spans: Vec<oxc_span::Span> = ingest.program.body.iter().map(|s| s.span()).collect();
        for row in humanify_core::modules::identity::scan_app_declarations(code, &spans) {
            let Some(name) = row.package else { continue };
            admit(name, row.version, row.reader.to_string());
        }
    }
    by_name.into_values().collect()
}

fn main() {
    for path in std::env::args().skip(1) {
        let code = std::fs::read_to_string(&path).expect("read bundle");
        let rows: Vec<serde_json::Value> = seeds(&code)
            .into_iter()
            .map(|(n, v, e)| serde_json::json!({"name": n, "version": v, "evidence": e}))
            .collect();
        println!("{}", serde_json::json!({"bundle": path, "seeds": rows}));
    }
}
