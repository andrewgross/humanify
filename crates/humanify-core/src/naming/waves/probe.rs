//! The naming graph's bisection probe (migration scaffolding — deleted at
//! phase 6): one JSON line per graph node in the shape of
//! test/parity/wp43-gen-probe.ts, so the two outputs join by id and every
//! graph-time field (node order, deps, callee insertion order, call sites,
//! the babel-printed function code / body / params, the module bindings'
//! prompt texts) is compared on the real texts.

use oxc_semantic::Semantic;
use serde_json::json;
use sha2::{Digest, Sha256};

use super::generate::TextView;
use super::graph_ext::{NamingGraph, NodeRef};
use super::render::FnPrinter;
use crate::graph::UnifiedGraph;

fn sha(text: &str) -> String {
    Sha256::digest(text.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// The probe rows (no renames applied: the graph-time names).
pub fn graph_probe_lines(
    semantic: &Semantic<'_>,
    graph: &UnifiedGraph,
    ng: &NamingGraph,
    view: &TextView<'_>,
    printer: &FnPrinter<'_, '_>,
    only: &[String],
) -> Vec<String> {
    let _ = semantic;
    let id = |n: usize| ng.session_id(graph, n).to_string();
    let mut out = Vec::new();
    out.push(
        json!({"k": "order", "ids": (0..ng.order.len()).map(id).collect::<Vec<_>>()}).to_string(),
    );
    let mut edges: Vec<String> = ng
        .scope_parent_edges
        .iter()
        .map(|(a, b)| format!("{}->{}", id(*a), id(*b)))
        .collect();
    edges.sort();
    out.push(json!({"k": "scopeEdges", "edges": edges}).to_string());
    for (n, node) in ng.order.iter().enumerate() {
        match *node {
            NodeRef::Fn(i) => {
                let code = printer.function_code(i);
                let body = printer.body_code(i);
                let params: Vec<String> = printer
                    .params(i)
                    .into_iter()
                    .filter_map(|p| match p {
                        crate::naming::context::ParamView::Other { code } => Some(code),
                        _ => None,
                    })
                    .collect();
                out.push(
                    json!({
                        "k": "fn",
                        "id": id(n),
                        "callees": ng.fn_callees[i].iter().map(|&c| id(ng.node_of_fn[c])).collect::<Vec<_>>(),
                        "scopeParent": ng.fn_scope_parent[i].map(|p| id(ng.node_of_fn[p])),
                        "deps": ng.deps[n].iter().map(|&d| id(d)).collect::<Vec<_>>(),
                        "callSites": ng.fn_call_sites[i],
                        "code": only.contains(&id(n)).then_some(&code),
                        "body": only.contains(&id(n)).then_some(&body),
                        "codeSha": sha(&code),
                        "bodySha": sha(&body),
                        "params": params,
                    })
                    .to_string(),
                );
            }
            NodeRef::Mb(j) => {
                let t = &ng.mb_text[j];
                out.push(
                    json!({
                        "k": "mb",
                        "id": id(n),
                        "line": t.declaration_line,
                        "declaration": t.declaration,
                        "assignments": t.assignments,
                        "usages": t.usages,
                        "deps": ng.deps[n].iter().map(|&d| id(d)).collect::<Vec<_>>(),
                    })
                    .to_string(),
                );
            }
        }
    }
    let _ = view;
    out
}
