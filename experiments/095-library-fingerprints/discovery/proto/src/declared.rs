//! exp095 discovery: the pipeline's own app-code readers over every unit.
//! Usage: exp095-declared <bundle.js> <raw.jsonl> > declared.jsonl
//! Each unit's span is extended back to the previous unit's end so a `/*!`
//! comment attached before the statement is inside the text the readers see.
use humanify_core::modules::identity::scan_app_declarations;
use oxc_span::Span;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let code = std::fs::read_to_string(&args[1]).expect("bundle");
    let mut spans: Vec<(u32, u32, bool)> = Vec::new();
    let mut cjs = std::collections::HashSet::new();
    let rows: Vec<serde_json::Value> = std::fs::read_to_string(&args[2])
        .expect("raw")
        .lines()
        .map(|l| serde_json::from_str(l).expect("row"))
        .collect();
    for r in &rows {
        if r["d"] == 0 && r["w"] == 1 && r["wk"] == "cjs" {
            cjs.insert(r["i"].as_i64().unwrap());
        }
    }
    for r in &rows {
        let wrapper = r["d"] == 0 && r["w"] == 1;
        if wrapper || (r["d"] == 1 && cjs.contains(&r["p"].as_i64().unwrap())) {
            continue;
        }
        spans.push((
            r["s"].as_u64().unwrap() as u32,
            r["e"].as_u64().unwrap() as u32,
            r["d"] == 1,
        ));
    }
    spans.sort();
    let mut prev = 0u32;
    let mut ext = Vec::with_capacity(spans.len());
    for &(s, e, _) in &spans {
        let start = if prev < s && prev > 0 { prev } else { s };
        ext.push(Span::new(start, e));
        prev = e;
    }
    for d in scan_app_declarations(&code, &ext) {
        println!("{}", serde_json::to_string(&d).unwrap());
    }
}
