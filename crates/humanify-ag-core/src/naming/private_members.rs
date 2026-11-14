//! Class-private member names (exp224): every minted `#name` gets a name
//! the class itself decides, so one class names its members the same way
//! in every release.
//!
//! A minifier re-letters private members on every build (`#a` in one
//! release is `#w` in the next), and nothing else in the pipeline names
//! them: they are not scope bindings, so the rename state, the model and
//! the prior transfer's slot tables never see them. Only the statement
//! twins carry a prior spelling, and only for a twin statement with
//! pending work. Every line that touched a private member therefore
//! changed between releases.
//!
//! The name is DERIVED, never asked: a pure function of what the class
//! says about the member in spellings a minifier keeps — property keys,
//! public member names, global constructors — and of the member's place
//! among its class's private members. A class that did not change names
//! its members identically in every release, with no prior needed and no
//! model draw to re-roll. In order, the first source that applies:
//!
//! 1. a public accessor that only returns it or only writes it
//!    (`get size() { return this.#a }` → `#size`);
//! 2. an object key whose value reads it (`{ keyMap: c.#a }` → `#keyMap`);
//! 3. an assignment from a property read (`this.#a = o.timeout ?? 5` →
//!    `#timeout`);
//! 4. a GLOBAL constructor it is built from (`#a = new Map()` → `#map`);
//!    a local class's name is the model's answer and can change, so it
//!    is not a source;
//! 5. otherwise its kind and ordinal among the class's private members of
//!    that kind, in declaration order (`#field2`, `#method1`,
//!    `#accessor1`).
//!
//! Within each source the first occurrence in source order wins. A
//! derived word that is not a legal rename target
//! (`rename::validated::target::is_valid_rename_target`), is
//! `constructor` (an early error as a private name) or is itself minted
//! falls through to the next source; a word already taken in the class
//! gets a numeric suffix (`#map2`).
//!
//! Only a member whose current name is minted (`rename::floor::
//! is_minifier_token` under the run's profile) is renamed: the author's
//! names, a library's restored names and a statement twin's carried
//! spelling are kept, and every kept name is taken. A private name is
//! invisible outside its class, so the only collisions are within the
//! class and with the classes nested in it: an inner class never takes a
//! name an enclosing class ends with (it would capture the outer
//! member's uses inside it), and an outer class never takes a name an
//! inner class keeps.
//!
//! Like the statement twins' rewrites (`naming::waves::render::
//! private_rename_edits`, which this pass applies first), the result is a
//! list of text edits the era's generate prints (docs/responsibility.md,
//! "Known exceptions": the `#` namespace has no validated path and cannot
//! hit a reserved word or a global).

use std::collections::{BTreeMap, HashMap, HashSet};

use oxc_ast::AstKind;
use oxc_ast::ast::{ClassElement, Expression, MethodDefinitionKind, PropertyKey};
use oxc_semantic::{NodeId, Semantic};
use oxc_span::{GetSpan, Span};

use crate::naming::waves::generate::Replacement;
use crate::naming::waves::render::private_rename_edits;
use crate::rename::floor::is_minifier_token;
use crate::rename::name_profile::NameProfile;
use crate::rename::validated::target::is_valid_rename_target;
use crate::twins::gates::PrivateRenameSet;

#[cfg(test)]
mod private_members_test;

/// What the pass did, per private member (a getter/setter pair is one
/// member).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct PrivateNameCounts {
    /// Private members declared by any class.
    pub members: usize,
    /// Kept: the current name is not minted.
    pub kept: usize,
    /// Renamed from a derivation source (accessor, key, read, constructor).
    pub derived: usize,
    /// Renamed by kind and ordinal.
    pub numbered: usize,
}

/// A member's kind, which names its ordinal fallback.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Field,
    Method,
    Accessor,
}

impl Kind {
    fn word(self) -> &'static str {
        match self {
            Kind::Field => "field",
            Kind::Method => "method",
            Kind::Accessor => "accessor",
        }
    }
}

/// A name source, strongest first (the module doc's order).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Source {
    Accessor,
    ObjectKey,
    ReadKey,
    Constructs,
}

/// One private member of one class.
struct Member {
    /// The current name (after the twins' rewrites).
    name: String,
    kind: Kind,
    /// 1-based ordinal among the class's members of this kind.
    ordinal: usize,
    /// Candidate words: (source, source position, word).
    evidence: Vec<(Source, u32, String)>,
    /// The final name, once decided.
    fin: Option<String>,
}

struct Class {
    span: Span,
    /// Members in declaration order.
    members: Vec<Member>,
    by_name: HashMap<String, usize>,
    /// The enclosing class, if any.
    parent: Option<usize>,
}

/// The private-member renames of a whole text, as edits (the twins'
/// rewrites included), plus what was done.
pub fn private_member_edits(
    semantic: &Semantic<'_>,
    twins: &[PrivateRenameSet],
    profile: NameProfile,
) -> (Vec<Replacement>, PrivateNameCounts) {
    let text = semantic.source_text();
    let twin_edits = private_rename_edits(text, twins);
    let current: HashMap<u32, String> = twin_edits
        .iter()
        .map(|r| (r.span.start, r.text[1..].to_string()))
        .collect();
    let name_at = |span: Span, name: &str| -> String {
        current
            .get(&span.start)
            .cloned()
            .unwrap_or_else(|| name.to_string())
    };
    let (mut classes, body_class) = collect_classes(semantic, &name_at);
    let occurrences = resolve_occurrences(semantic, &classes, &body_class, &name_at);
    for occ in &occurrences {
        if let Some(ev) = evidence_at(semantic, occ.node) {
            classes[occ.class].members[occ.member].evidence.push(ev);
        }
    }
    collect_initializer_evidence(semantic, &mut classes, &body_class, &name_at);
    let counts = decide_names(&mut classes, profile);
    let mut edits: BTreeMap<u32, Replacement> =
        twin_edits.into_iter().map(|r| (r.span.start, r)).collect();
    for occ in &occurrences {
        let fin = classes[occ.class].members[occ.member]
            .fin
            .as_deref()
            .expect("every member is decided");
        let original = &text[occ.span.start as usize + 1..occ.span.end as usize];
        if fin == original {
            edits.remove(&occ.span.start);
        } else {
            edits.insert(
                occ.span.start,
                Replacement {
                    span: occ.span,
                    text: format!("#{fin}"),
                },
            );
        }
    }
    (edits.into_values().collect(), counts)
}

/// A class's private member declarations: (current name, kind), in order.
type Declared = Vec<(String, Kind)>;

/// Every class with its private members, in source order, and the map
/// from a class BODY node to its class index.
fn collect_classes(
    semantic: &Semantic<'_>,
    name_at: &dyn Fn(Span, &str) -> String,
) -> (Vec<Class>, HashMap<NodeId, usize>) {
    let nodes = semantic.nodes();
    let mut found: Vec<(Span, NodeId, Declared)> = Vec::new();
    for node in nodes.iter() {
        let AstKind::Class(class) = node.kind() else {
            continue;
        };
        let mut members: Declared = Vec::new();
        for element in &class.body.body {
            let (key, kind) = match element {
                ClassElement::MethodDefinition(m) => (
                    &m.key,
                    match m.kind {
                        MethodDefinitionKind::Get | MethodDefinitionKind::Set => Kind::Accessor,
                        _ => Kind::Method,
                    },
                ),
                ClassElement::PropertyDefinition(p) => (&p.key, Kind::Field),
                ClassElement::AccessorProperty(a) => (&a.key, Kind::Accessor),
                _ => continue,
            };
            if let PropertyKey::PrivateIdentifier(p) = key {
                members.push((name_at(p.span, &p.name), kind));
            }
        }
        found.push((class.span, class.body.node_id(), members));
    }
    found.sort_by_key(|(span, _, _)| (span.start, std::cmp::Reverse(span.end)));
    let mut classes: Vec<Class> = Vec::with_capacity(found.len());
    let mut body_class: HashMap<NodeId, usize> = HashMap::new();
    let mut stack: Vec<usize> = Vec::new();
    for (span, body, declared) in found {
        while stack
            .last()
            .is_some_and(|&top| classes[top].span.end <= span.start)
        {
            stack.pop();
        }
        let mut class = Class {
            span,
            members: Vec::new(),
            by_name: HashMap::new(),
            parent: stack.last().copied(),
        };
        let mut per_kind: HashMap<&'static str, usize> = HashMap::new();
        for (name, kind) in declared {
            if class.by_name.contains_key(&name) {
                continue; // the second half of a getter/setter pair
            }
            let ordinal = per_kind.entry(kind.word()).or_default();
            *ordinal += 1;
            class.by_name.insert(name.clone(), class.members.len());
            class.members.push(Member {
                name,
                kind,
                ordinal: *ordinal,
                evidence: Vec::new(),
                fin: None,
            });
        }
        body_class.insert(body, classes.len());
        stack.push(classes.len());
        classes.push(class);
    }
    (classes, body_class)
}

/// One `#name` node resolved to the member it names.
struct Occurrence {
    node: NodeId,
    span: Span,
    class: usize,
    member: usize,
}

/// Resolve every private identifier to the innermost enclosing class body
/// that declares its (current) name. An unresolved one (no declaring
/// class: not valid JavaScript) is left alone.
fn resolve_occurrences(
    semantic: &Semantic<'_>,
    classes: &[Class],
    body_class: &HashMap<NodeId, usize>,
    name_at: &dyn Fn(Span, &str) -> String,
) -> Vec<Occurrence> {
    let nodes = semantic.nodes();
    let mut out = Vec::new();
    for node in nodes.iter() {
        let AstKind::PrivateIdentifier(p) = node.kind() else {
            continue;
        };
        let name = name_at(p.span, &p.name);
        let resolved = nodes.ancestor_ids(node.id()).find_map(|id| {
            let class = *body_class.get(&id)?;
            classes[class]
                .by_name
                .get(&name)
                .map(|&member| (class, member))
        });
        if let Some((class, member)) = resolved {
            out.push(Occurrence {
                node: node.id(),
                span: p.span,
                class,
                member,
            });
        }
    }
    out
}

/// The name source a USE of a private member offers, if any.
fn evidence_at(semantic: &Semantic<'_>, private: NodeId) -> Option<(Source, u32, String)> {
    let nodes = semantic.nodes();
    let field_id = nodes.parent_id(private);
    let AstKind::PrivateFieldExpression(field) = nodes.kind(field_id) else {
        return None;
    };
    let field_span = field.span;
    let on_this = matches!(field.object, Expression::ThisExpression(_));
    let parent_id = nodes.parent_id(field_id);
    match nodes.kind(parent_id) {
        AstKind::ObjectProperty(op) if !op.computed && op.value.span() == field_span => {
            static_key(&op.key).map(|k| (Source::ObjectKey, op.span.start, k))
        }
        AstKind::ReturnStatement(_) if on_this => {
            accessor_name(semantic, parent_id, MethodDefinitionKind::Get)
                .map(|k| (Source::Accessor, field_span.start, k))
        }
        AstKind::AssignmentExpression(a) if on_this && a.left.span() == field_span => {
            if a.operator.is_assign() {
                let statement = nodes.parent_id(parent_id);
                if matches!(nodes.kind(statement), AstKind::ExpressionStatement(_))
                    && let Some(k) = accessor_name(semantic, statement, MethodDefinitionKind::Set)
                {
                    return Some((Source::Accessor, field_span.start, k));
                }
            }
            value_evidence(semantic, &a.right).map(|(s, k)| (s, a.span.start, k))
        }
        _ => None,
    }
}

/// The public accessor of `kind` whose body is exactly the statement
/// `statement` (`get size() { return this.#a }`), if any.
fn accessor_name(
    semantic: &Semantic<'_>,
    statement: NodeId,
    kind: MethodDefinitionKind,
) -> Option<String> {
    let nodes = semantic.nodes();
    let body_id = nodes.parent_id(statement);
    let AstKind::FunctionBody(body) = nodes.kind(body_id) else {
        return None;
    };
    if body.statements.len() != 1 {
        return None;
    }
    let function_id = nodes.parent_id(body_id);
    if !matches!(nodes.kind(function_id), AstKind::Function(_)) {
        return None;
    }
    match nodes.kind(nodes.parent_id(function_id)) {
        AstKind::MethodDefinition(m) if m.kind == kind && !m.computed => static_key(&m.key),
        _ => None,
    }
}

/// What an assigned value says: a property read (`o.timeout`, also behind
/// `??` / `||` / `&&`) names it by the property; a global constructor by
/// the constructor.
fn value_evidence(semantic: &Semantic<'_>, value: &Expression<'_>) -> Option<(Source, String)> {
    match value.without_parentheses() {
        Expression::StaticMemberExpression(m) => {
            Some((Source::ReadKey, m.property.name.to_string()))
        }
        Expression::LogicalExpression(l) => match l.left.without_parentheses() {
            Expression::StaticMemberExpression(m) => {
                Some((Source::ReadKey, m.property.name.to_string()))
            }
            _ => None,
        },
        Expression::NewExpression(n) => {
            let Expression::Identifier(callee) = &n.callee else {
                return None;
            };
            let reference = semantic.scoping().get_reference(callee.reference_id());
            if reference.symbol_id().is_some() {
                return None;
            }
            Some((Source::Constructs, lower_camel(&callee.name)))
        }
        _ => None,
    }
}

/// A field initializer is evidence too (`#a = new Map()`).
fn collect_initializer_evidence(
    semantic: &Semantic<'_>,
    classes: &mut [Class],
    body_class: &HashMap<NodeId, usize>,
    name_at: &dyn Fn(Span, &str) -> String,
) {
    for node in semantic.nodes().iter() {
        let AstKind::ClassBody(body) = node.kind() else {
            continue;
        };
        let Some(&class) = body_class.get(&node.id()) else {
            continue;
        };
        for element in &body.body {
            let ClassElement::PropertyDefinition(p) = element else {
                continue;
            };
            let (PropertyKey::PrivateIdentifier(key), Some(value)) = (&p.key, &p.value) else {
                continue;
            };
            let name = name_at(key.span, &key.name);
            let Some(&member) = classes[class].by_name.get(&name) else {
                continue;
            };
            if let Some((source, word)) = value_evidence(semantic, value) {
                classes[class].members[member]
                    .evidence
                    .push((source, p.span.start, word));
            }
        }
    }
}

/// A non-computed key's spelling (`foo` or `"foo"`).
fn static_key(key: &PropertyKey<'_>) -> Option<String> {
    match key {
        PropertyKey::StaticIdentifier(id) => Some(id.name.to_string()),
        PropertyKey::StringLiteral(s) => Some(s.value.to_string()),
        _ => None,
    }
}

/// `Map` → `map`, `AbortController` → `abortController`, `URL` → `url`,
/// `URLSearchParams` → `urlSearchParams`.
fn lower_camel(name: &str) -> String {
    let chars: Vec<char> = name.chars().collect();
    let upper_run = chars.iter().take_while(|c| c.is_ascii_uppercase()).count();
    let lower_to = if upper_run == chars.len() || upper_run <= 1 {
        upper_run
    } else {
        upper_run - 1
    };
    chars
        .iter()
        .enumerate()
        .map(|(i, c)| {
            if i < lower_to {
                c.to_ascii_lowercase()
            } else {
                *c
            }
        })
        .collect()
}

/// May `word` be a derived private name?
fn usable_word(word: &str, profile: NameProfile) -> bool {
    word != "constructor" && is_valid_rename_target(word) && !is_minifier_token(profile, word)
}

/// Decide every member's final name: classes outermost first, members in
/// declaration order.
fn decide_names(classes: &mut [Class], profile: NameProfile) -> PrivateNameCounts {
    let mut counts = PrivateNameCounts::default();
    for c in 0..classes.len() {
        // Names this class may not take: its kept names, every name an
        // enclosing class ended with, every name a nested class keeps.
        let mut taken: HashSet<String> = HashSet::new();
        let mut ancestor = classes[c].parent;
        while let Some(a) = ancestor {
            taken.extend(classes[a].members.iter().filter_map(|m| m.fin.clone()));
            ancestor = classes[a].parent;
        }
        let span = classes[c].span;
        for inner in classes[c + 1..]
            .iter()
            .take_while(|k| k.span.start < span.end)
        {
            taken.extend(
                inner
                    .members
                    .iter()
                    .filter(|m| !is_minifier_token(profile, &m.name))
                    .map(|m| m.name.clone()),
            );
        }
        let class = &mut classes[c];
        for m in &mut class.members {
            counts.members += 1;
            if !is_minifier_token(profile, &m.name) {
                counts.kept += 1;
                taken.insert(m.name.clone());
                m.fin = Some(m.name.clone());
            }
        }
        for m in &mut class.members {
            if m.fin.is_some() {
                continue;
            }
            m.evidence.sort();
            let word = m
                .evidence
                .iter()
                .map(|(_, _, w)| w.as_str())
                .find(|w| usable_word(w, profile));
            let name = match word {
                Some(w) => {
                    counts.derived += 1;
                    let mut n = 1;
                    loop {
                        let candidate = if n == 1 {
                            w.to_string()
                        } else {
                            format!("{w}{n}")
                        };
                        if !taken.contains(&candidate) {
                            break candidate;
                        }
                        n += 1;
                    }
                }
                None => {
                    counts.numbered += 1;
                    let mut n = m.ordinal;
                    loop {
                        let candidate = format!("{}{n}", m.kind.word());
                        if !taken.contains(&candidate) {
                            break candidate;
                        }
                        n += 1;
                    }
                }
            };
            taken.insert(name.clone());
            m.fin = Some(name);
        }
    }
    counts
}
