//! fpq: the QUERY side of the library index, for the lab only.
//!
//! For each bundle in a job list (`slice<TAB>out.js<TAB>out.js.map|-<TAB>result.jsonl`), one JSON row per
//! CommonJS module the pipeline's module walk finds (`bundle_content_keys`, the exact function humanify-index
//! uses to build module rows): its span, its content key, its literal set in Bundled mode (`libindex::literals`,
//! the extractor the index's literal rows come from, in Source mode), and — when a sourcemap is given — its
//! owner by humanify-index's own attribution (>= 90% of mapping segments from one source file; the nearest
//! package.json under the last node_modules/). A module with no owner records its source path (app code).
use std::collections::HashMap;
use std::io::Write;
use std::path::Path;

use humanify_core::libindex::IndexSlice;
use humanify_core::libindex::literals::{LiteralMode, code_literals};
use humanify_index::sourcemap::{SourceMap, owner_cached, utf16_to_byte_offsets};
use rayon::prelude::*;
use serde_json::json;

fn literals(body: &str) -> Option<Vec<String>> {
    code_literals(&format!("({body})"), LiteralMode::Bundled)
        .or_else(|| code_literals(body, LiteralMode::Bundled))
        .map(|s| s.into_iter().collect())
}

fn one(slice: &str, js: &str, map: &str, out: &str) -> Result<usize, String> {
    let slice = IndexSlice::from_name(slice).ok_or("unknown slice")?;
    let code = std::fs::read_to_string(js).map_err(|e| format!("{js}: {e}"))?;
    let toolchain = humanify_core::toolchain::resolve_toolchain(
        &humanify_core::detect::detect_bundle(&code),
        Some(slice.bundler()),
        None,
    );
    let modules = humanify_core::modules::vendor_content::bundle_content_keys(
        &code,
        toolchain.module_wrappers.piece,
        toolchain.layout.piece,
    );
    let spans: Vec<(u32, u32)> = modules
        .iter()
        .map(|m| (m.body_span.start, m.body_span.end))
        .collect();
    let (srcs, map) = if map != "-" {
        let text = std::fs::read_to_string(map).map_err(|e| format!("{map}: {e}"))?;
        let m = SourceMap::parse(&text)?;
        let pure = m.attribute(&code, &spans, 0.9);
        let major = m.attribute(&code, &spans, 0.5);
        (pure.into_iter().zip(major).collect::<Vec<_>>(), Some(m))
    } else {
        (vec![(None, None); spans.len()], None)
    };
    let out_dir = Path::new(js).parent().unwrap_or(Path::new("."));
    let mut owners = HashMap::new();
    if let Some(mp) = &map {
        // every byte of the bundle to the source of the mapping segment it follows (whole-bundle truth)
        let pos: Vec<(u32, u32)> = mp.segments.iter().map(|&(l, c, _)| (l, c)).collect();
        let mut at: Vec<(usize, u32)> = utf16_to_byte_offsets(&code, &pos)
            .into_iter()
            .zip(&mp.segments)
            .filter_map(|(o, &(_, _, s))| o.map(|o| (o, s)))
            .collect();
        at.sort_unstable();
        let mut per: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
        let unmapped = at.first().map(|a| a.0).unwrap_or(code.len());
        *per.entry("(unmapped)".into()).or_default() += unmapped;
        for (k, &(o, s)) in at.iter().enumerate() {
            let end = at.get(k + 1).map(|n| n.0).unwrap_or(code.len());
            let src = &mp.sources[s as usize];
            let who = match owner_cached(out_dir, src, &mut owners) {
                Some(o) => format!("{}@{}", o.package, o.version),
                None => format!("app:{src}"),
            };
            *per.entry(who).or_default() += end - o;
        }
        std::fs::write(
            format!("{out}.owners.json"),
            serde_json::to_string(&per).unwrap_or_default(),
        )
        .map_err(|e| e.to_string())?;
    }
    let mut f = std::io::BufWriter::new(std::fs::File::create(out).map_err(|e| e.to_string())?);
    for (i, (m, src)) in modules.iter().zip(srcs).enumerate() {
        let (s, e) = (m.body_span.start as usize, m.body_span.end as usize);
        let lits = literals(&code[s..e]);
        let mut row = json!({"i": i, "start": s, "end": e, "key": m.key, "lits": lits});
        let (src, major) = src;
        if let (Some(mp), None, Some(mj)) = (&map, src, major) {
            // a mixed span: its majority source's owner, for the reader to judge (not a truth)
            if let Some(o) = owner_cached(out_dir, &mp.sources[mj as usize], &mut owners) {
                row["majorOwner"] = json!(o.package);
            }
        }
        if let (Some(mp), Some(src)) = (&map, src) {
            let source = &mp.sources[src as usize];
            match owner_cached(out_dir, source, &mut owners) {
                Some(o) => {
                    row["owner"] = json!(o.package);
                    row["version"] = json!(o.version);
                    row["file"] = json!(o.file);
                }
                None => row["source"] = json!(source),
            }
        } else if map.is_some() {
            row["mixed"] = json!(true);
        }
        writeln!(f, "{row}").map_err(|e| e.to_string())?;
    }
    Ok(modules.len())
}

fn main() {
    let list = std::env::args().nth(1).expect("usage: fpq <jobs.tsv>");
    let jobs: Vec<Vec<String>> = std::fs::read_to_string(&list)
        .expect("jobs")
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.split('\t').map(str::to_string).collect())
        .collect();
    jobs.par_iter().for_each(|j| {
        let r = std::panic::catch_unwind(|| one(&j[0], &j[1], &j[2], &j[3]));
        match r {
            Ok(Ok(n)) => println!("ok\t{}\t{n}", j[3]),
            Ok(Err(e)) => println!("err\t{}\t{}", j[3], e.replace('\n', " ")),
            Err(_) => println!("panic\t{}", j[3]),
        }
    });
}
