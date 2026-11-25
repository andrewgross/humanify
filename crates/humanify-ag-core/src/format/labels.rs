//! Names for statement labels: a label is named for the statement it
//! marks, whatever the minifier called it.

use super::ast::{Kind, NodeId, Tree};

/// The name for a label on `body`.
fn base(kind: &Kind) -> &'static str {
    match kind {
        Kind::ForStatement { .. }
        | Kind::ForInStatement { .. }
        | Kind::ForOfStatement { .. }
        | Kind::WhileStatement { .. }
        | Kind::DoWhileStatement { .. } => "loop",
        Kind::SwitchStatement { .. } => "choice",
        _ => "block",
    }
}

/// `base`, numbered from 2 when a label around it has the name.
fn free(base: &str, open: &[(String, String)]) -> String {
    let taken = |n: &str| open.iter().any(|(_, new)| new == n);
    if !taken(base) {
        return base.to_string();
    }
    (2..)
        .map(|k| format!("{base}{k}"))
        .find(|n| !taken(n))
        .expect("an unused number")
}

enum Step {
    Enter(NodeId),
    /// Leave a labelled statement.
    Close,
    /// Leave a function: its labels are its own.
    Restore(Vec<(String, String)>),
}

fn rename(tree: &mut Tree, id: NodeId, to: &str) {
    if let Kind::Identifier { name } = tree.kind_mut(id) {
        *name = to.to_string();
    }
}

/// Rename every label under `root`, with the `break` and `continue`
/// statements that name it.
pub fn name_labels(tree: &mut Tree, root: NodeId) {
    // (written name, new name) of each label around the current node.
    let mut open: Vec<(String, String)> = Vec::new();
    let mut work = vec![Step::Enter(root)];
    while let Some(step) = work.pop() {
        let id = match step {
            Step::Enter(id) => id,
            Step::Close => {
                open.pop();
                continue;
            }
            Step::Restore(outer) => {
                open = outer;
                continue;
            }
        };
        let target = match tree.kind(id) {
            Kind::BreakStatement { label: Some(l) }
            | Kind::ContinueStatement { label: Some(l) } => Some(*l),
            _ => None,
        };
        if let Some(l) = target {
            let written = tree.kind(l).identifier_name().map(str::to_string);
            let new = open
                .iter()
                .rev()
                .find(|(old, _)| Some(old) == written.as_ref())
                .map(|(_, new)| new.clone());
            if let Some(new) = new {
                rename(tree, l, &new);
            }
            continue;
        }
        if let Kind::LabeledStatement { label, body } = tree.kind(id) {
            let (label, body) = (*label, *body);
            let written = tree.kind(label).identifier_name().unwrap_or("").to_string();
            let new = free(base(tree.kind(body)), &open);
            rename(tree, label, &new);
            open.push((written, new));
            work.push(Step::Close);
            work.push(Step::Enter(body));
            continue;
        }
        if tree.kind(id).is_function() {
            work.push(Step::Restore(std::mem::take(&mut open)));
        }
        for child in tree.children(id).into_iter().rev() {
            work.push(Step::Enter(child));
        }
    }
}
