//! The IDENTITY-TRANSFORM registry (exp200): every rewrite a hash walk may
//! apply to make two SPELLINGS of the same code hash equal — the plug point
//! for "treat these ops the same" (Andrew, 2026-11-07).
//!
//! A minifier picks a spelling for code it did not write: an arrow body
//! concise or braced, a module wrapper as an arrow or a function
//! expression. When the packaging tool's style moves between releases,
//! every module whose spelling moved changes its hash although its source
//! did not (2.1.179 -> 2.1.181: about 130 vendor modules at once,
//! experiments/200-identity-transforms). An identity transform says, for
//! one such spelling pair, "these mean the same thing" — and the hash walks
//! serialize both under ONE canonical spelling.
//!
//! The rules every entry keeps:
//!
//! - **Semantics-preserving under its condition.** A transform fires on a
//!   node only when the rewrite cannot change what the code does, and the
//!   condition is checked PER NODE from the node's own subtree (the hash
//!   never sees how a value is used elsewhere). Each entry documents its
//!   condition and the residual it leaves to census, as
//!   `hash::wrapper_spelling` does.
//! - **One owner per question.** The condition lives in ONE function that
//!   every walk arm reads; a walk never re-implements it.
//! - **Forms opt in.** A [`super::forms::CanonicalForm`] declares its
//!   [`TransformSet`]; a transform reaches no form that does not list it.
//!   Adding a transform to a form, or changing a transform's rule, is a NEW
//!   VERSION of every form that lists it (the persisted forms' readers
//!   refuse a mismatched stamp — exp093's lesson).
//!
//! Adding a transform = one [`Transform`] variant, one [`TRANSFORMS`] entry
//! (name, version, condition, the walks that implement it), its rule in
//! [`Transform::rewrite`], its tests, and the version bump of each form that
//! lists it. A transform rewrites one node in one of two ways ([`Rewrite`]):
//! it renames the node's TYPE token (no copy — the wrapper spelling, which
//! fires on module-sized wrappers), or it hands the walk a small REPLACEMENT
//! node that the walk serializes in the original's place (the concise arrow
//! body). Both walks that carry transforms — the canonical serializer and
//! the statement hash — apply the same [`Rewrite`].
//!
//! NAME equivalences are not transforms: which names a form ignores
//! (locals, class-private names, free names) are the form's own settings
//! (`Locals`, `Privates`, `FreeNames`). A transform is about structure.

use serde_json::{Map, Value};

use super::forms::Walk;

/// One registered identity transform.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Transform {
    /// exp094: a SAFE arrow walks under the `FunctionExpression` token.
    WrapperSpelling,
    /// exp200: a concise arrow body `x => E` walks as `x => { return E; }`.
    ConciseArrowBody,
}

/// A registry entry: what a transform is, the condition it fires under,
/// and which walks implement it.
#[derive(Clone, Copy, Debug)]
pub struct TransformSpec {
    pub transform: Transform,
    pub name: &'static str,
    /// The rule's era: a change to the rule is a new version of the
    /// transform AND of every form that lists it.
    pub version: u64,
    /// The semantics-preservation condition, in one line (the full
    /// argument is the rule's own doc).
    pub condition: &'static str,
    /// The walks that apply it; a form on any other walk may not list it.
    pub walks: &'static [Walk],
}

/// Every identity transform, in the order a walk tries them.
pub const TRANSFORMS: [TransformSpec; 2] = [
    TransformSpec {
        transform: Transform::ConciseArrowBody,
        name: "concise-arrow-body",
        version: 1,
        condition: "always: a concise body evaluates its expression and returns it \
                    (ECMA-262 ConciseBody); parentheses around it are grouping only",
        walks: &[Walk::Canonical, Walk::Statement],
    },
    TransformSpec {
        transform: Transform::WrapperSpelling,
        name: "wrapper-spelling",
        version: 1,
        condition: "a braced arrow whose own lexical scope reads no this / arguments / \
                    new.target (hash::wrapper_spelling)",
        walks: &[Walk::Canonical, Walk::Statement],
    },
];

impl Transform {
    /// This transform's registry entry.
    pub fn spec(self) -> &'static TransformSpec {
        TRANSFORMS
            .iter()
            .find(|s| s.transform == self)
            .expect("every Transform has a TRANSFORMS entry")
    }

    const fn bit(self) -> u32 {
        match self {
            Transform::WrapperSpelling => 1,
            Transform::ConciseArrowBody => 2,
        }
    }

    /// What this transform does to one node (`node_type` is its `type`).
    pub(crate) fn rewrite(self, node_type: &str, map: &Map<String, Value>) -> Rewrite {
        match self {
            Transform::WrapperSpelling => {
                if node_type == "ArrowFunctionExpression"
                    && super::wrapper_spelling::arrow_serializes_as_function(map)
                {
                    Rewrite::TypeToken("FunctionExpression")
                } else {
                    Rewrite::Keep
                }
            }
            Transform::ConciseArrowBody => {
                if node_type == "ArrowFunctionExpression" {
                    concise_arrow_as_block(map).map_or(Rewrite::Keep, Rewrite::Node)
                } else {
                    Rewrite::Keep
                }
            }
        }
    }
}

/// What a transform does to one node.
#[derive(Debug, PartialEq)]
pub(crate) enum Rewrite {
    /// Nothing: the node walks as written.
    Keep,
    /// The node walks under this TYPE token (its fields untouched).
    TypeToken(&'static str),
    /// The walk serializes this node in the original's place (and tries
    /// every transform again on it).
    Node(Value),
}

/// A set of transforms — what a form declares.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransformSet(u32);

impl TransformSet {
    pub const NONE: TransformSet = TransformSet(0);

    pub const fn of(transforms: &[Transform]) -> TransformSet {
        let mut bits = 0;
        let mut i = 0;
        while i < transforms.len() {
            bits |= transforms[i].bit();
            i += 1;
        }
        TransformSet(bits)
    }

    pub const fn contains(self, t: Transform) -> bool {
        self.0 & t.bit() != 0
    }

    /// The set's transforms, in registry order.
    pub fn iter(self) -> impl Iterator<Item = Transform> {
        TRANSFORMS
            .iter()
            .map(|s| s.transform)
            .filter(move |t| self.contains(*t))
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Run the set's transforms on one node, in registry order: the first
    /// REPLACEMENT wins (the walk re-enters with it, so the rest run on the
    /// replacement), else the first type-token rename, else keep.
    pub(crate) fn rewrite(self, node_type: &str, map: &Map<String, Value>) -> Rewrite {
        if self.is_empty() {
            return Rewrite::Keep;
        }
        let mut token = None;
        for t in self.iter() {
            match t.rewrite(node_type, map) {
                Rewrite::Keep => {}
                Rewrite::Node(v) => return Rewrite::Node(v),
                Rewrite::TypeToken(name) => {
                    token.get_or_insert(name);
                }
            }
        }
        token.map_or(Rewrite::Keep, Rewrite::TypeToken)
    }
}

/// The concise-arrow-body rule (exp200): an arrow with an EXPRESSION body
/// as the same arrow with the braced body `{ return E; }` — `E` with the
/// parentheses the concise position forced around it removed
/// (`x => ({})`, `x => (a, b)`). None for a braced arrow.
///
/// The condition is unconditional: ECMA-262 evaluates a ConciseBody by
/// evaluating the expression and returning its value, exactly what a
/// `return` of it does, and a concise body has no directive prologue to
/// lose. The rewrite only ever moves the concise spelling onto the braced
/// one, so the braced side's stream is unchanged; with the wrapper-spelling
/// transform also on, the braced result is then judged by that rule's own
/// condition like any braced arrow.
///
/// The replacement copies the arrow's fields and the body expression (the
/// identifiers keep their `start`, so the walk resolves them to the same
/// symbols and slot ordinals: the body still precedes the params in the
/// field order). Only the concise body is copied, never a whole wrapper.
fn concise_arrow_as_block(map: &Map<String, Value>) -> Option<Value> {
    if map.get("expression").and_then(Value::as_bool) != Some(true) {
        return None;
    }
    let mut body = map.get("body")?;
    if body.get("type").and_then(Value::as_str) == Some("BlockStatement") {
        return None;
    }
    while body.get("type").and_then(Value::as_str) == Some("ParenthesizedExpression") {
        body = body.get("expression")?;
    }
    let mut ret = Map::new();
    ret.insert("type".into(), Value::from("ReturnStatement"));
    ret.insert("argument".into(), body.clone());
    let mut block = Map::new();
    block.insert("type".into(), Value::from("BlockStatement"));
    block.insert("body".into(), Value::Array(vec![Value::Object(ret)]));
    // A field copy of the arrow minus its body (the one field replaced).
    let mut out: Map<String, Value> = map
        .iter()
        .filter(|(k, _)| k.as_str() != "body")
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    out.insert("body".into(), Value::Object(block));
    out.insert("expression".into(), Value::Bool(false));
    Some(Value::Object(out))
}
