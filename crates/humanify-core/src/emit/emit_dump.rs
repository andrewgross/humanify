//! The emit section of the `--dump-artifacts` catalog: the emitted
//! layout as `emit.json` rows (span-keyed into the shipped text).

use humanify_model::dump::{EmitFileRow, EmitLayoutFile, EmitStatement, SpanKey};
use humanify_model::js::cmp_utf16;

/// The layout rows, sorted by path as the dump writer sorts them.
pub fn layout_rows(
    layout: &[(String, Vec<usize>)],
    spans: &[(u32, u32)],
    alias_of: impl Fn(&str) -> Option<String>,
) -> EmitLayoutFile {
    let mut files: Vec<EmitFileRow> = layout
        .iter()
        .map(|(path, idxs)| EmitFileRow {
            path: path.clone(),
            alias: alias_of(path),
            statements: idxs
                .iter()
                .enumerate()
                .map(|(slot, &i)| EmitStatement {
                    span: SpanKey {
                        text: "shipped".into(),
                        start: i64::from(spans[i].0),
                        end: i64::from(spans[i].1),
                    },
                    slot_index: slot as u64,
                    bundle_index: i as u64,
                })
                .collect(),
        })
        .collect();
    files.sort_by(|a, b| cmp_utf16(&a.path, &b.path));
    EmitLayoutFile {
        schema_version: 1,
        files,
    }
}
