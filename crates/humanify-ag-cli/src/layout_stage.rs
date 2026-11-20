//! Writes the run's layout record (`.humanify/layout.json`) once every
//! file of the tree has its final lines.

use std::path::Path;

use humanify_ag_core::emit::stable_split::SplitOutcome;
use humanify_ag_core::layout_record::LAYOUT_PATH;
use humanify_ag_core::layout_record::write::{
    Input, LayoutError, Previous, Run, Split, Text, layout_of,
};
use humanify_ag_core::output_tree::OutputTree;
use humanify_ag_core::prior_tree::PriorTree;
use humanify_ag_core::rename::transfer::carry::PriorCarry;

/// What the formatter recorded for the named file.
#[derive(Default)]
pub struct FormatMarks {
    /// `(offset in the formatted text, offset in the unpacked file)`.
    pub statements: Vec<(u32, u32)>,
    pub depths: Vec<u32>,
    pub functions: Vec<(u32, Option<u32>)>,
}

/// What the stages before the split recorded for the layout record.
#[derive(Clone, Copy)]
pub struct LayoutInput<'a> {
    pub link: Option<&'a humanify_ag_core::unpack::chunk_link::LinkMap>,
    pub unpack: Option<&'a humanify_ag_core::unpack::bun::UnpackOrigins>,
    pub marks: &'a FormatMarks,
    /// The formatted text the naming stage ran on.
    pub fresh: &'a str,
    pub input_file: &'a Path,
    pub prior: Option<&'a PriorTree>,
}

fn line_ends(tree: &OutputTree, path: &Path) -> u32 {
    tree.read(path).map_or(0, |bytes| {
        bytes.iter().filter(|b| **b == b'\n').count() as u32
    })
}

/// Write the layout of a tree whose named text (`shipped`) sits at
/// `named`; `split`: the files the split wrote from it.
pub fn write_layout(
    input: &LayoutInput<'_>,
    shipped: &str,
    named: &Path,
    carry: Option<&PriorCarry>,
    split: Option<&SplitOutcome>,
    tree: &mut OutputTree,
) -> Result<(), LayoutError> {
    let root = tree.root().to_path_buf();
    let name = |p: &Path| p.file_name().map(|n| n.to_string_lossy().into_owned());
    let named_text = match named.strip_prefix(&root) {
        Ok(rel) => rel.to_string_lossy().replace('\\', "/"),
        Err(_) => name(named).unwrap_or_default(),
    };
    let input_name = name(input.input_file).unwrap_or_default();
    let line_count = |path: &str| line_ends(tree, &root.join(path));
    let run = Run {
        input: Input {
            link: input.link,
            name: &input_name,
            unpack: input.unpack,
        },
        text: Text {
            fresh: input.fresh,
            shipped,
            statements: &input.marks.statements,
            depths: &input.marks.depths,
            functions: &input.marks.functions,
            written_line_ends: line_ends(tree, named),
        },
        previous: carry.map(|carry| Previous {
            matches: &carry.decisions,
            layout: input.prior.and_then(|p| p.files.text(LAYOUT_PATH)),
        }),
        named_text: &named_text,
        split: split.map(|outcome| Split {
            spans: &outcome.spans,
            files: &outcome.lines,
            line_count: &line_count,
        }),
    };
    let text = layout_of(&run)?.to_json();
    tree.put_rel(LAYOUT_PATH, text);
    Ok(())
}
