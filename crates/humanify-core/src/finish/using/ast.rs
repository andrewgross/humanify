//! A Babel-shaped, OWNED AST for the `using` desugar: what
//! `@babel/generator` prints and what the explicit-resource-management
//! transform mutates. Built from oxc's AST ([`super::convert`]); every
//! variant is ONE Babel node type, because the generator's decisions
//! (parentheses, token context, `isLastChild`) key on Babel's types and
//! child fields, not oxc's.
//!
//! Only what the generator reads is kept: `loc` as start/end LINES
//! (`retainLines` catches up to them), `extra.raw` for literals,
//! `extra.parenthesized`, and `_compact` (helper nodes). Nodes the
//! transform synthesizes carry no `loc`, exactly like Babel's template
//! nodes.

/// A node's source lines (`node.loc.start.line` / `node.loc.end.line`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Loc {
    pub start: u32,
    pub end: u32,
}

/// One Babel node.
#[derive(Clone, Debug)]
pub struct Node {
    pub kind: Kind,
    pub loc: Option<Loc>,
    /// `extra.parenthesized`.
    pub parenthesized: bool,
    /// `node._compact` (set on injected helper nodes).
    pub compact: bool,
}

pub type P = Box<Node>;

/// A function's shared fields (FunctionDeclaration / FunctionExpression /
/// ObjectMethod / ClassMethod / ClassPrivateMethod / Arrow).
#[derive(Clone, Debug)]
pub struct Func {
    pub id: Option<P>,
    pub params: Vec<Node>,
    pub body: P,
    pub is_async: bool,
    pub generator: bool,
}

/// A method's key fields (ObjectMethod / ClassMethod / ClassPrivateMethod).
#[derive(Clone, Debug)]
pub struct Method {
    /// "method" / "get" / "set" / "constructor".
    pub kind: &'static str,
    pub key: P,
    pub computed: bool,
    pub is_static: bool,
    pub func: Func,
}

/// A class field (ClassProperty / ClassPrivateProperty / ClassAccessorProperty).
#[derive(Clone, Debug)]
pub struct Field {
    pub key: P,
    pub value: Option<P>,
    pub computed: bool,
    pub is_static: bool,
}

#[derive(Clone, Debug)]
pub struct Class {
    pub id: Option<P>,
    pub super_class: Option<P>,
    pub body: P,
}

#[derive(Clone, Debug)]
pub struct Call {
    pub callee: P,
    pub arguments: Vec<Node>,
    /// OptionalCallExpression's own `optional`.
    pub optional: bool,
}

#[derive(Clone, Debug)]
pub struct Member {
    pub object: P,
    pub property: P,
    pub computed: bool,
    /// OptionalMemberExpression's own `optional`.
    pub optional: bool,
}

#[derive(Clone, Debug)]
pub struct Binary {
    pub operator: &'static str,
    pub left: P,
    pub right: P,
}

#[derive(Clone, Debug)]
pub enum Kind {
    Program {
        interpreter: Option<String>,
        directives: Vec<Node>,
        body: Vec<Node>,
    },
    Directive {
        value: P,
    },
    DirectiveLiteral {
        raw: String,
    },
    BlockStatement {
        directives: Vec<Node>,
        body: Vec<Node>,
    },
    StaticBlock {
        body: Vec<Node>,
    },
    ExpressionStatement {
        expression: P,
    },
    EmptyStatement,
    DebuggerStatement,
    WithStatement {
        object: P,
        body: P,
    },
    ReturnStatement {
        argument: Option<P>,
    },
    ThrowStatement {
        argument: P,
    },
    LabeledStatement {
        label: P,
        body: P,
    },
    BreakStatement {
        label: Option<P>,
    },
    ContinueStatement {
        label: Option<P>,
    },
    IfStatement {
        test: P,
        consequent: P,
        alternate: Option<P>,
    },
    SwitchStatement {
        discriminant: P,
        cases: Vec<Node>,
    },
    SwitchCase {
        test: Option<P>,
        consequent: Vec<Node>,
    },
    TryStatement {
        block: P,
        handler: Option<P>,
        finalizer: Option<P>,
    },
    CatchClause {
        param: Option<P>,
        body: P,
    },
    WhileStatement {
        test: P,
        body: P,
    },
    DoWhileStatement {
        body: P,
        test: P,
    },
    ForStatement {
        init: Option<P>,
        test: Option<P>,
        update: Option<P>,
        body: P,
    },
    ForInStatement {
        left: P,
        right: P,
        body: P,
    },
    ForOfStatement {
        is_await: bool,
        left: P,
        right: P,
        body: P,
    },
    FunctionDeclaration(Func),
    FunctionExpression(Func),
    ArrowFunctionExpression(Func),
    VariableDeclaration {
        /// "var" / "let" / "const" / "using" / "await using".
        kind: &'static str,
        declarations: Vec<Node>,
    },
    VariableDeclarator {
        id: P,
        init: Option<P>,
    },
    ClassDeclaration(Class),
    ClassExpression(Class),
    ClassBody {
        body: Vec<Node>,
    },
    ClassMethod(Method),
    ClassPrivateMethod(Method),
    ClassProperty(Field),
    ClassPrivateProperty(Field),
    ClassAccessorProperty(Field),
    Identifier {
        name: String,
    },
    PrivateName {
        id: P,
    },
    StringLiteral {
        value: String,
        raw: Option<String>,
    },
    NumericLiteral {
        value: f64,
        raw: Option<String>,
    },
    BigIntLiteral {
        raw: String,
    },
    BooleanLiteral {
        value: bool,
    },
    NullLiteral,
    RegExpLiteral {
        /// `/${pattern}/${flags}`.
        text: String,
    },
    TemplateLiteral {
        /// Each quasi's `value.raw` (CRLF normalized, as Babel's parser).
        quasis: Vec<String>,
        expressions: Vec<Node>,
    },
    TaggedTemplateExpression {
        tag: P,
        quasi: P,
    },
    ThisExpression,
    Super,
    Import,
    ArrayExpression {
        elements: Vec<Option<Node>>,
    },
    ArrayPattern {
        elements: Vec<Option<Node>>,
    },
    ObjectExpression {
        properties: Vec<Node>,
    },
    ObjectPattern {
        properties: Vec<Node>,
    },
    ObjectProperty {
        key: P,
        value: P,
        computed: bool,
        shorthand: bool,
    },
    ObjectMethod(Method),
    SpreadElement {
        argument: P,
    },
    RestElement {
        argument: P,
    },
    UnaryExpression {
        operator: &'static str,
        argument: P,
    },
    UpdateExpression {
        operator: &'static str,
        prefix: bool,
        argument: P,
    },
    BinaryExpression(Binary),
    LogicalExpression(Binary),
    AssignmentExpression(Binary),
    AssignmentPattern {
        left: P,
        right: P,
    },
    ConditionalExpression {
        test: P,
        consequent: P,
        alternate: P,
    },
    CallExpression(Call),
    NewExpression(Call),
    OptionalCallExpression(Call),
    MemberExpression(Member),
    OptionalMemberExpression(Member),
    SequenceExpression {
        expressions: Vec<Node>,
    },
    YieldExpression {
        argument: Option<P>,
        delegate: bool,
    },
    AwaitExpression {
        argument: P,
    },
    MetaProperty {
        meta: P,
        property: P,
    },
}

impl Node {
    pub fn new(kind: Kind, loc: Option<Loc>) -> Node {
        Node {
            kind,
            loc,
            parenthesized: false,
            compact: false,
        }
    }

    /// A synthesized node (no `loc`), as a Babel template or builder makes.
    pub fn synth(kind: Kind) -> Node {
        Node::new(kind, None)
    }

    pub fn ident(name: &str) -> Node {
        Node::synth(Kind::Identifier {
            name: name.to_string(),
        })
    }

    /// Babel's `isStatement` (the `Statement` alias: statements and
    /// declarations).
    pub fn is_statement(&self) -> bool {
        matches!(
            self.kind,
            Kind::BlockStatement { .. }
                | Kind::StaticBlock { .. }
                | Kind::ExpressionStatement { .. }
                | Kind::EmptyStatement
                | Kind::DebuggerStatement
                | Kind::WithStatement { .. }
                | Kind::ReturnStatement { .. }
                | Kind::ThrowStatement { .. }
                | Kind::LabeledStatement { .. }
                | Kind::BreakStatement { .. }
                | Kind::ContinueStatement { .. }
                | Kind::IfStatement { .. }
                | Kind::SwitchStatement { .. }
                | Kind::TryStatement { .. }
                | Kind::WhileStatement { .. }
                | Kind::DoWhileStatement { .. }
                | Kind::ForStatement { .. }
                | Kind::ForInStatement { .. }
                | Kind::ForOfStatement { .. }
                | Kind::FunctionDeclaration(_)
                | Kind::VariableDeclaration { .. }
                | Kind::ClassDeclaration(_)
        )
    }

    /// Babel's `isFunction` alias.
    pub fn is_function(&self) -> bool {
        matches!(
            self.kind,
            Kind::FunctionDeclaration(_)
                | Kind::FunctionExpression(_)
                | Kind::ArrowFunctionExpression(_)
                | Kind::ObjectMethod(_)
                | Kind::ClassMethod(_)
                | Kind::ClassPrivateMethod(_)
        )
    }

    /// The function's shared fields, when this is a Babel function.
    pub fn func(&self) -> Option<&Func> {
        match &self.kind {
            Kind::FunctionDeclaration(f)
            | Kind::FunctionExpression(f)
            | Kind::ArrowFunctionExpression(f) => Some(f),
            Kind::ObjectMethod(m) | Kind::ClassMethod(m) | Kind::ClassPrivateMethod(m) => {
                Some(&m.func)
            }
            _ => None,
        }
    }

    pub fn is_identifier(&self) -> bool {
        matches!(self.kind, Kind::Identifier { .. })
    }

    pub fn identifier_name(&self) -> Option<&str> {
        match &self.kind {
            Kind::Identifier { name } => Some(name),
            _ => None,
        }
    }

    /// The child slots in Babel's `VISITOR_KEYS` order — the traversal
    /// order, and `isLastChild`'s walk. A `Many` slot is an array field.
    pub fn slots(&self) -> Vec<Slot<'_>> {
        use Slot::{Many, ManyOpt, One};
        match &self.kind {
            Kind::Program {
                directives, body, ..
            } => vec![Many(directives), Many(body)],
            Kind::Directive { value } => vec![One(Some(value))],
            Kind::BlockStatement { directives, body } => vec![Many(directives), Many(body)],
            Kind::StaticBlock { body } => vec![Many(body)],
            Kind::ExpressionStatement { expression } => vec![One(Some(expression))],
            Kind::WithStatement { object, body } => vec![One(Some(object)), One(Some(body))],
            Kind::ReturnStatement { argument } => vec![one(argument)],
            Kind::ThrowStatement { argument } => vec![One(Some(argument))],
            Kind::LabeledStatement { label, body } => vec![One(Some(label)), One(Some(body))],
            Kind::BreakStatement { label } | Kind::ContinueStatement { label } => vec![one(label)],
            Kind::IfStatement {
                test,
                consequent,
                alternate,
            } => vec![One(Some(test)), One(Some(consequent)), one(alternate)],
            Kind::SwitchStatement {
                discriminant,
                cases,
            } => vec![One(Some(discriminant)), Many(cases)],
            Kind::SwitchCase { test, consequent } => vec![one(test), Many(consequent)],
            Kind::TryStatement {
                block,
                handler,
                finalizer,
            } => vec![One(Some(block)), one(handler), one(finalizer)],
            Kind::CatchClause { param, body } => vec![one(param), One(Some(body))],
            Kind::WhileStatement { test, body } => vec![One(Some(test)), One(Some(body))],
            Kind::DoWhileStatement { body, test } => vec![One(Some(body)), One(Some(test))],
            Kind::ForStatement {
                init,
                test,
                update,
                body,
            } => vec![one(init), one(test), one(update), One(Some(body))],
            Kind::ForInStatement { left, right, body }
            | Kind::ForOfStatement {
                left, right, body, ..
            } => vec![One(Some(left)), One(Some(right)), One(Some(body))],
            Kind::FunctionDeclaration(func) | Kind::FunctionExpression(func) => {
                vec![one(&func.id), Many(&func.params), One(Some(&func.body))]
            }
            Kind::ArrowFunctionExpression(func) => {
                vec![Many(&func.params), One(Some(&func.body))]
            }
            Kind::VariableDeclaration { declarations, .. } => vec![Many(declarations)],
            Kind::VariableDeclarator { id, init } => vec![One(Some(id)), one(init)],
            Kind::ClassDeclaration(c) | Kind::ClassExpression(c) => {
                vec![one(&c.id), one(&c.super_class), One(Some(&c.body))]
            }
            Kind::ClassBody { body } => vec![Many(body)],
            Kind::ClassMethod(m) | Kind::ClassPrivateMethod(m) | Kind::ObjectMethod(m) => vec![
                One(Some(&m.key)),
                Many(&m.func.params),
                One(Some(&m.func.body)),
            ],
            Kind::ClassProperty(fd)
            | Kind::ClassPrivateProperty(fd)
            | Kind::ClassAccessorProperty(fd) => {
                vec![One(Some(&fd.key)), one(&fd.value)]
            }
            Kind::PrivateName { id } => vec![One(Some(id))],
            Kind::TemplateLiteral { expressions, .. } => vec![Many(expressions)],
            Kind::TaggedTemplateExpression { tag, quasi } => vec![One(Some(tag)), One(Some(quasi))],
            Kind::ArrayExpression { elements } | Kind::ArrayPattern { elements } => {
                vec![ManyOpt(elements)]
            }
            Kind::ObjectExpression { properties } | Kind::ObjectPattern { properties } => {
                vec![Many(properties)]
            }
            Kind::ObjectProperty { key, value, .. } => vec![One(Some(key)), One(Some(value))],
            Kind::SpreadElement { argument } | Kind::RestElement { argument } => {
                vec![One(Some(argument))]
            }
            Kind::UnaryExpression { argument, .. } | Kind::UpdateExpression { argument, .. } => {
                vec![One(Some(argument))]
            }
            Kind::BinaryExpression(b)
            | Kind::LogicalExpression(b)
            | Kind::AssignmentExpression(b) => {
                vec![One(Some(&b.left)), One(Some(&b.right))]
            }
            Kind::AssignmentPattern { left, right } => vec![One(Some(left)), One(Some(right))],
            Kind::ConditionalExpression {
                test,
                consequent,
                alternate,
            } => vec![One(Some(test)), One(Some(consequent)), One(Some(alternate))],
            Kind::CallExpression(c) | Kind::NewExpression(c) | Kind::OptionalCallExpression(c) => {
                vec![One(Some(&c.callee)), Many(&c.arguments)]
            }
            Kind::MemberExpression(m) | Kind::OptionalMemberExpression(m) => {
                vec![One(Some(&m.object)), One(Some(&m.property))]
            }
            Kind::SequenceExpression { expressions } => vec![Many(expressions)],
            Kind::YieldExpression { argument, .. } => vec![one(argument)],
            Kind::AwaitExpression { argument } => vec![One(Some(argument))],
            Kind::MetaProperty { meta, property } => vec![One(Some(meta)), One(Some(property))],
            Kind::DirectiveLiteral { .. }
            | Kind::EmptyStatement
            | Kind::DebuggerStatement
            | Kind::Identifier { .. }
            | Kind::StringLiteral { .. }
            | Kind::NumericLiteral { .. }
            | Kind::BigIntLiteral { .. }
            | Kind::BooleanLiteral { .. }
            | Kind::NullLiteral
            | Kind::RegExpLiteral { .. }
            | Kind::ThisExpression
            | Kind::Super
            | Kind::Import => vec![],
        }
    }

    /// `isLastChild(parent, child)` (generator node/index.ts): from the last
    /// visitor key backwards, the first PRESENT field decides — an array
    /// field by its last non-null element (an empty array answers false).
    pub fn is_last_child(&self, child: &Node) -> bool {
        for slot in self.slots().into_iter().rev() {
            match slot {
                Slot::One(None) => continue,
                Slot::One(Some(n)) => return std::ptr::eq(n, child),
                Slot::Many(v) => return v.last().is_some_and(|n| std::ptr::eq(n, child)),
                Slot::ManyOpt(v) => {
                    return v
                        .iter()
                        .rev()
                        .find_map(|n| n.as_ref())
                        .is_some_and(|n| std::ptr::eq(n, child));
                }
            }
        }
        false
    }
}

fn one(p: &Option<P>) -> Slot<'_> {
    Slot::One(p.as_deref())
}

/// One visitor-key field of a node.
pub enum Slot<'n> {
    One(Option<&'n Node>),
    Many(&'n [Node]),
    ManyOpt(&'n [Option<Node>]),
}
