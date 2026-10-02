//! The wrapper-spelling equivalence rule (exp094) — the ONE oxc-ESTree
//! owner of "is flipping this function's spelling semantics-preserving?".
//!
//! An arrow and a function expression are NOT universally interchangeable:
//! the arrow has no own `this`, `arguments`, `new.target` or `prototype`,
//! while the function does. "Arrow == function" is therefore a CONDITIONAL
//! equivalence, and the conditions are checked HERE, per node, from the
//! node's own subtree — the hash never sees how the function value is used
//! elsewhere, so the `.prototype`/`new`-on-the-value residual is bounded by
//! census, not by construction (experiments/094-wrapper-spelling: every
//! walk-tree flip is a call argument to ONE receiver that only calls it,
//! and no arrow in the walked corpus is ever a `new` callee).
//!
//! TWO Rust hash arms read this module — the whole point of keeping it one
//! rule:
//!
//! - the MatchKey families (`crate::hash::serialize`: function
//!   fingerprints, statement contexts, twin gates, factory hashes) — there
//!   a safe arrow walks under the `FunctionExpression` TOKEN;
//! - the statement hash (`crate::hash::statement_hash`: split inheritance,
//!   statement twins, family permute) — there a safe arrow's node line
//!   walks under the `FunctionExpression` token too (exp094b; every
//!   [`crate::hash::statement_hash::STATEMENT_HASH_VERSION`] consumer).
//!
//! DIRECTION: only the ARROW's bytes change. A safe function expression's
//! stream was ALREADY the unified spelling (oxc's ESTree gives both forms
//! the identical field set `async, body, expression:false, generator:false,
//! params` — the type token was the only difference), so the function-side
//! conditions (`id`, `generator`, its own `this`) need no gates in the
//! MatchKey walk: an id, `generator:true` or a `MetaProperty` is in the
//! bytes themselves, and a loaded arrow refuses its unified spelling, so a
//! loaded PAIR keeps both spellings — and its real difference — apart. The
//! statement hash's stream is coarser (node types + content, no scalars):
//! there the function-side head fields must ride as node CONTENT
//! ([`function_head_fields`]) and a binding `id` rides as a child node, so
//! the same conditions hold in both arms without re-implementing anything.
//!
//! The TS instruments' arm of the same rule is
//! `experiments/lib/js/wrapper-spelling.ts` (babel ASTs; the 037 detector
//! and match-truth's head erasure) — a change to one arm is a change to
//! all of them (docs/responsibility.md's declaration row).

use serde_json::Value;

/// Does this ArrowFunctionExpression serialize under the FunctionExpression
/// token? The flip arrow -> function expression changes MEANING unless the
/// arrow's own lexical scope observes a binding the flip rebinds — so the
/// walk refuses on `this`/`arguments`/`new.target` in the arrow's own
/// scope (occurrences behind a nested classic function, class shell, class
/// field initializer or static block are bound THERE and do not refuse),
/// and a concise arrow body (not re-spellable without restructuring) stays
/// as-is. `async` is carried as a field by both spellings and is not a
/// condition.
pub(crate) fn arrow_serializes_as_function(map: &serde_json::Map<String, Value>) -> bool {
    let body_type = map
        .get("body")
        .and_then(|b| b.get("type"))
        .and_then(|t| t.as_str());
    body_type == Some("BlockStatement") && !arrow_owns_outer_binding_use(map)
}

/// The function-head fields the unified spelling must carry so the
/// function-SIDE conditions of the rule hold in the statement hash's
/// scalar-free stream: `async` (condition 3 — it must MATCH, it is carried
/// as a field) and `generator` (condition 2 — a `function*` never unifies;
/// its MatchKey bytes carry `generator:true` for free). `""` when neither.
/// Only the statement hash's [`node_content`] consumes this — the MatchKey
/// walk serializes the scalars themselves.
pub(crate) fn function_head_fields(map: &serde_json::Map<String, Value>) -> &'static str {
    let is_async = map.get("async").and_then(Value::as_bool).unwrap_or(false);
    let generator = map
        .get("generator")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    match (is_async, generator) {
        (true, true) => "async+generator",
        (true, false) => "async",
        (false, true) => "generator",
        (false, false) => "",
    }
}

/// Node types whose bodies run under their own `this`/`arguments` (or, for
/// static blocks and class field initializers, the class's/instance's): an
/// occurrence behind one of these cannot observe the flipped arrow's
/// binding. Arrows are deliberately absent — they pass both through. (The
/// exp037 detector's `LEXICAL_BINDERS` is the same rule; see the exp094
/// README for the two deliberate widenings recorded here.)
fn is_lexical_binder(node_type: &str) -> bool {
    matches!(
        node_type,
        "FunctionExpression"
            | "FunctionDeclaration"
            | "ClassDeclaration"
            | "ClassExpression"
            | "StaticBlock"
            | "PropertyDefinition"
    )
}

/// Fields of a node whose children evaluate in the OUTER scope even though
/// the node binds: a class's `extends` clause, and computed keys (method
/// and field names evaluate at definition time, in the enclosing scope).
fn is_outer_scope_field(parent_type: &str, key: &str, computed: bool) -> bool {
    match (parent_type, key) {
        ("ClassDeclaration", "superClass") | ("ClassExpression", "superClass") => true,
        ("MethodDefinition", "key") | ("PropertyDefinition", "key") | ("Property", "key") => {
            computed
        }
        _ => false,
    }
}

/// Fields that hold an `arguments` NON-reference: a non-computed member
/// property (`x.arguments`), a non-computed object key (shorthand's VALUE
/// carries the reference), and label names.
fn is_arguments_non_reference(parent_type: &str, key: &str, computed: bool) -> bool {
    match (parent_type, key) {
        ("MemberExpression", "property") | ("OptionalMemberExpression", "property") => !computed,
        ("Property", "key") | ("PropertyDefinition", "key") | ("MethodDefinition", "key") => {
            !computed
        }
        ("LabeledStatement", "label")
        | ("BreakStatement", "label")
        | ("ContinueStatement", "label") => true,
        _ => false,
    }
}

/// Does the node itself observe the flipped arrow's binding?
fn node_uses_outer_bindings(map: &serde_json::Map<String, Value>, node_type: &str) -> bool {
    if node_type == "ThisExpression" {
        return true;
    }
    if node_type == "MetaProperty" {
        // `new.target` (`import.meta` evaluates identically in both
        // spellings and does not refuse).
        return map
            .get("meta")
            .and_then(|m| m.get("name"))
            .and_then(|v| v.as_str())
            == Some("new");
    }
    node_type == "Identifier" && map.get("name").and_then(|v| v.as_str()) == Some("arguments")
}

/// Where a value sits while it is pushed onto the scope walk's stack: the
/// parent node's type, the field name, the parent's computed-ness, and
/// whether a lexical binder already encloses it.
#[derive(Clone, Copy)]
struct ScopeCtx<'a> {
    parent_type: &'a str,
    key: &'a str,
    computed: bool,
    barrier: bool,
}

/// Push one value onto the scope walk — arrays flatten, non-node scalars
/// drop. The child's barrier flips to true when it IS a lexical binder, to
/// false when it sits in an outer-scope field (extends/computed key).
fn push_scope_frames<'a>(
    stack: &mut Vec<(&'a Value, ScopeCtx<'a>)>,
    value: &'a Value,
    ctx: ScopeCtx<'a>,
) {
    let child_type = value
        .as_object()
        .and_then(|m| m.get("type"))
        .and_then(|t| t.as_str())
        .unwrap_or("");
    let barrier = if is_outer_scope_field(ctx.parent_type, ctx.key, ctx.computed) {
        false
    } else if is_lexical_binder(child_type) {
        true
    } else {
        ctx.barrier
    };
    let child_ctx = ScopeCtx { barrier, ..ctx };
    match value {
        Value::Array(items) => {
            for item in items {
                push_scope_frames(stack, item, child_ctx);
            }
        }
        Value::Object(_) if !child_type.is_empty() => stack.push((value, child_ctx)),
        _ => {}
    }
}

/// Any `this`/`arguments`/`new.target` in the ARROW's own lexical scope —
/// ones the flip to a function expression would rebind? Iterative (explicit
/// stack) like the walk in statement_hash, for the multi-thousand-line
/// wrapper bodies.
fn arrow_owns_outer_binding_use(map: &serde_json::Map<String, Value>) -> bool {
    let mut stack: Vec<(&Value, ScopeCtx<'_>)> = Vec::new();
    // The arrow's own params (default values evaluate under the arrow's
    // binding) and body.
    for key in ["params", "body"] {
        if let Some(v) = map.get(key) {
            push_scope_frames(
                &mut stack,
                v,
                ScopeCtx {
                    parent_type: "ArrowFunctionExpression",
                    key,
                    computed: false,
                    barrier: false,
                },
            );
        }
    }
    while let Some((value, ctx)) = stack.pop() {
        let Some(m) = value.as_object() else {
            continue;
        };
        let Some(node_type) = m.get("type").and_then(|t| t.as_str()) else {
            continue;
        };
        if !ctx.barrier && node_uses_outer_bindings(m, node_type) {
            return true;
        }
        let computed = m.get("computed").and_then(|c| c.as_bool()).unwrap_or(false);
        for (k, v) in m.iter() {
            // An `arguments` non-reference (member property, non-computed
            // object key, label name) is not walked — nothing observable
            // can be inside it.
            if is_arguments_non_reference(node_type, k, computed) {
                continue;
            }
            push_scope_frames(
                &mut stack,
                v,
                ScopeCtx {
                    parent_type: node_type,
                    key: k,
                    computed,
                    barrier: ctx.barrier,
                },
            );
        }
    }
    false
}
