//! The hash families (WP1.3): the canonical serialization's three consumers
//! — `MatchKey` (blurred literals; cross-version matching), `IdentityKey`
//! (verbatim literals; the declaration-body hash), and the STATEMENT hash
//! (the split's rename-invariant statement identity, its own masked walk).
//! TS originals: analysis/structural-hash.ts, split/statement-hash.ts,
//! analysis/enclosing-statement.ts.

pub mod serialize;
pub mod statement_hash;

/// The WP1.3 gate's Rust-side partition dump: ingest a TS dump's shipped
/// text, hash every wrapper statement, and emit a Rust-side dump dir the
/// differ can compare (`compare --sections partitions`).
///
/// Partition, not bytes: the digests differ by design (different
/// serialization); the differ compares the equivalence classes (07 §4).
/// Spans need no conversion: oxc spans ARE UTF-8 byte offsets, and the TS
/// dumper converted its own to bytes — the R1 census showed the statement
/// boundaries agree, so the joins line up.
pub mod partition_dump {
    use std::fs;
    use std::path::Path;

    use oxc_allocator::Allocator;
    use serde_json::{Value, json};

    use crate::hash::statement_hash::{STATEMENT_HASH_VERSION, statement_hash};
    use crate::ingest::Ingest;

    /// Produce `<out>/meta.json` + `<out>/partitions.json` from the TS
    /// dump's anchors: meta is the TS dump's own (the Rust side's claims
    /// must compare equal), and the two hash families are rebuilt.
    pub fn dump_partitions(ts_dump_dir: &Path, out_dir: &Path) -> Result<usize, String> {
        let meta_text = fs::read_to_string(ts_dump_dir.join("meta.json"))
            .map_err(|e| format!("meta.json: {e}"))?;
        let meta: Value =
            serde_json::from_str(&meta_text).map_err(|e| format!("meta.json parse: {e}"))?;

        // ── the statementHash family ────────────────────────────────────
        // shipped.js may be absent (fixture dumps carry no split) — the
        // statementHash family is then absent on BOTH sides (the TS wrote
        // only the families whose anchor text exists).
        let shipped_path = ts_dump_dir.join("text").join("shipped.js");
        let shipped = fs::read_to_string(&shipped_path).ok();
        let statement_members: Vec<Value> = shipped
            .as_deref()
            .map(|shipped| -> Result<Vec<Value>, String> {
                let allocator = Allocator::default();
                let ingest = Ingest::parse(&allocator, shipped, "shipped.js");
                if !ingest.errors.is_empty() {
                    return Err(format!(
                        "oxc failed to parse the shipped text: {} diagnostic(s)",
                        ingest.errors.len()
                    ));
                }
                // Every wrapper-body statement, in bundle order, hashed with
                // the Rust statement hash. The ESTree JSON substrate per
                // statement: serialize the PROGRAM once (one JSON), then walk
                // its wrapper body statements — spans in the JSON carry byte
                // offsets, so the members key like the TS side's converted
                // spans.
                let estree = ingest.program.to_estree_json(false, true);
                // The AST nests hundreds deep; serde_json's default recursion
                // limit (128) rejects it. Unbounded depth is safe here: the
                // input is oxc's own serialization of a program that parsed.
                let mut de = serde_json::Deserializer::from_str(&estree);
                de.disable_recursion_limit();
                let program_json: Value = serde::Deserialize::deserialize(&mut de)
                    .map_err(|e| format!("estree json: {e}"))?;
                // The split's unit = the WRAPPER BODY's statements, not the
                // program's: descend the top-level expression statement into
                // the IIFE call/arrow's body (unwrapping oxc's parenthesized
                // expressions — R1's finding).
                let statements = wrapper_body_statements(&program_json)
                    .ok_or("no wrapper body found in the estree json")?;
                let mut members: Vec<Value> = Vec::with_capacity(statements.len());
                for stmt in statements {
                    let start = stmt
                        .get("start")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(u64::MAX);
                    let end = stmt.get("end").and_then(|v| v.as_u64()).unwrap_or(u64::MAX);
                    let hash = statement_hash(stmt);
                    members.push(json!({
                        "member": {"text": "shipped", "start": start, "end": end},
                        "hash": hash
                    }));
                }
                members.sort_by(|a, b| {
                    let key = |v: &Value| {
                        (
                            v["member"]["start"].as_u64().unwrap_or(u64::MAX),
                            v["member"]["end"].as_u64().unwrap_or(u64::MAX),
                        )
                    };
                    key(a).cmp(&key(b))
                });
                Ok(members)
            })
            .transpose()?
            .unwrap_or_default();

        // ── the structuralHash family ───────────────────────────────────
        // The graph's per-row fingerprints: function rows' structural
        // hashes + the module-binding content fingerprints, anchored on
        // FRESH (the graph rows' key text). The classes — not the bytes —
        // are what the gate compares (02 §4a).
        let fresh = fs::read_to_string(ts_dump_dir.join("text").join("fresh.js"))
            .map_err(|e| format!("fresh text: {e}"))?;
        let fresh_allocator = Allocator::default();
        let fresh_ingest = Ingest::parse(&fresh_allocator, &fresh, "fresh.js");
        if !fresh_ingest.errors.is_empty() {
            return Err(format!(
                "oxc failed to parse the fresh text: {} diagnostic(s)",
                fresh_ingest.errors.len()
            ));
        }
        let meta_flags = &meta["flags"];
        let unified = crate::graph::build_unified_graph(
            &fresh_ingest.semantic,
            fresh_ingest.program,
            "input.js",
            &[],
            meta_flags["bundler"].as_str(),
            meta_flags["minifier"].as_str(),
        );
        let mut family_members: Vec<Value> = Vec::new();
        for f in &unified.functions {
            family_members.push(json!({
                "member": {"text": "fresh", "start": f.span.start, "end": f.span.end},
                "hash": f.structural_hash
            }));
        }
        for mb in &unified.module_bindings {
            if let Some(hash) = &mb.fingerprint_hash {
                family_members.push(json!({
                    "member": {"text": "fresh", "start": mb.span.start, "end": mb.span.end},
                    "hash": hash
                }));
            }
        }
        family_members.sort_by(|a, b| {
            let key = |v: &Value| {
                (
                    v["member"]["start"].as_u64().unwrap_or(u64::MAX),
                    v["member"]["end"].as_u64().unwrap_or(u64::MAX),
                )
            };
            key(a).cmp(&key(b))
        });

        fs::create_dir_all(out_dir.join("text")).map_err(|e| format!("mkdir: {e}"))?;
        fs::write(
            out_dir.join("meta.json"),
            serde_json::to_string(&meta).unwrap(),
        )
        .map_err(|e| format!("write meta: {e}"))?;
        if let Some(shipped) = &shipped {
            fs::write(out_dir.join("text").join("shipped.js"), shipped)
                .map_err(|e| format!("write text: {e}"))?;
        }
        // The statementHash family is written only when its anchor text
        // exists — the TS writes only the families whose anchor exists.
        let mut families = vec![json!({
            "family": "structuralHash",
            "members": family_members
        })];
        if shipped.is_some() {
            families.push(json!({
                "family": "statementHash",
                "members": statement_members
            }));
        }
        fs::write(
            out_dir.join("partitions.json"),
            serde_json::to_string(&json!({
                "schemaVersion": 1,
                "families": families
            }))
            .unwrap(),
        )
        .map_err(|e| format!("write partitions: {e}"))?;
        let _ = STATEMENT_HASH_VERSION;
        Ok(family_members.len() + statement_members.len())
    }

    /// The wrapper body's statements: program.body[0] as an expression
    /// statement whose expression (parenthesized as needed) is a call or
    /// arrow/function whose body is a block.
    fn wrapper_body_statements(program: &Value) -> Option<&Vec<Value>> {
        let stmt = program.get("body")?.as_array()?.first()?;
        let mut expr = stmt.get("expression")?;
        while expr.get("type").and_then(|t| t.as_str()) == Some("ParenthesizedExpression") {
            expr = expr.get("expression")?;
        }
        let inner = match expr.get("type").and_then(|t| t.as_str()) {
            Some("CallExpression") => {
                let mut callee = expr.get("callee")?;
                while callee.get("type").and_then(|t| t.as_str()) == Some("ParenthesizedExpression")
                {
                    callee = callee.get("expression")?;
                }
                match callee.get("type").and_then(|t| t.as_str()) {
                    Some("ArrowFunctionExpression") | Some("FunctionExpression") => {
                        callee.get("body")?
                    }
                    _ => return None,
                }
            }
            Some("ArrowFunctionExpression") | Some("FunctionExpression") => expr.get("body")?,
            _ => return None,
        };
        if inner.get("type").and_then(|t| t.as_str()) != Some("BlockStatement") {
            return None;
        }
        inner.get("body")?.as_array()
    }
}

#[cfg(test)]
mod statement_hash_test;
