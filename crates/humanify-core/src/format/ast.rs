//! The Babel-shaped AST `core::format` owns: what `@babel/generator`
//! prints, what the beautify transforms and the `using` desugar mutate.
//!
//! It is an ARENA ([`Tree`]): nodes live in one `Vec` and refer to their
//! children by [`NodeId`]. That is Babel's object graph, not a tree of
//! owned boxes, and the difference is load-bearing — the beautifier's
//! visitors SHARE nodes (`val = c ? a : b` builds two assignments over the
//! one `val` Identifier) and keep traversing a DETACHED node's children
//! after its parent statement was replaced (a LogicalExpression whose
//! ExpressionStatement just became an IfStatement still visits `left` and
//! `right` through its own, now stale, path). Nothing is ever freed.
//!
//! Every variant is ONE Babel node type, because the generator's decisions
//! (parentheses, token context, `isLastChild`) and the traversal's
//! (`VISITOR_KEYS`, `isStatement`) key on Babel's types and child fields,
//! not oxc's. Kept per node: `loc` as start/end LINES (retainLines catches
//! up to them), the source span (the 5.6c extension point: the library
//! carry classifies functions by their RAW start — finding #32),
//! `extra.parenthesized`, `_compact` (helper nodes), and the attached
//! comments ([`NodeComments`]: never printed with `comments: false`, but
//! they still decide parentheses — see `printer`). Synthesized nodes carry
//! none of them, exactly like Babel's builders.

/// A node's index in its [`Tree`]. `NONE` is a hole: an array elision, or
/// the slot `replaceWithMultiple` nulls before it inserts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(pub u32);

impl NodeId {
    pub const NONE: NodeId = NodeId(u32::MAX);

    pub fn is_none(self) -> bool {
        self == NodeId::NONE
    }

    pub fn opt(self) -> Option<NodeId> {
        if self.is_none() { None } else { Some(self) }
    }
}

/// A node's source lines (`node.loc.start.line` / `node.loc.end.line`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Loc {
    pub start: u32,
    pub end: u32,
}

/// One comment of the input (Babel's `CommentBlock` / `CommentLine`).
#[derive(Clone, Debug)]
pub struct Comment {
    pub block: bool,
    /// The text between the delimiters (`comment.value`).
    pub value: String,
    pub start: u32,
    pub end: u32,
    pub loc: Loc,
}

/// `leadingComments` / `trailingComments` / `innerComments`, as indices
/// into [`Tree::comments`].
#[derive(Clone, Debug, Default)]
pub struct NodeComments {
    pub leading: Vec<u32>,
    pub trailing: Vec<u32>,
    pub inner: Vec<u32>,
}

impl NodeComments {
    pub fn is_empty(&self) -> bool {
        self.leading.is_empty() && self.trailing.is_empty() && self.inner.is_empty()
    }
}

/// One Babel node.
#[derive(Clone, Debug)]
pub struct Node {
    pub kind: Kind,
    pub loc: Option<Loc>,
    /// The node's source span (byte offsets into the parsed text) — None
    /// for a synthesized node. EXTENSION POINT (WP5.6c): the library carry
    /// reads a function's RAW start from here (the TS reads
    /// `fn.path.node.start` on the beautify OUTPUT tree, whose function
    /// nodes are the raw parse's — finding #32).
    pub span: Option<(u32, u32)>,
    /// `extra.parenthesized`.
    pub parenthesized: bool,
    /// `node._compact` (set on injected helper nodes).
    pub compact: bool,
    pub comments: Option<Box<NodeComments>>,
}

/// A function's shared fields (FunctionDeclaration / FunctionExpression /
/// Arrow, and a method's function part).
#[derive(Clone, Debug)]
pub struct Func {
    pub id: Option<NodeId>,
    pub params: Vec<NodeId>,
    pub body: NodeId,
    pub is_async: bool,
    pub generator: bool,
}

/// A method (ObjectMethod / ClassMethod / ClassPrivateMethod).
#[derive(Clone, Debug)]
pub struct Method {
    /// "method" / "get" / "set" / "constructor".
    pub kind: &'static str,
    pub key: NodeId,
    pub computed: bool,
    pub is_static: bool,
    pub func: Func,
}

/// A class field (ClassProperty / ClassPrivateProperty / ClassAccessorProperty).
#[derive(Clone, Debug)]
pub struct Prop {
    pub key: NodeId,
    pub value: Option<NodeId>,
    pub computed: bool,
    pub is_static: bool,
}

#[derive(Clone, Debug)]
pub struct Class {
    pub id: Option<NodeId>,
    pub super_class: Option<NodeId>,
    pub body: NodeId,
}

#[derive(Clone, Debug)]
pub struct Call {
    pub callee: NodeId,
    pub arguments: Vec<NodeId>,
    /// OptionalCallExpression's own `optional`.
    pub optional: bool,
}

#[derive(Clone, Debug)]
pub struct Member {
    pub object: NodeId,
    pub property: NodeId,
    pub computed: bool,
    /// OptionalMemberExpression's own `optional`.
    pub optional: bool,
}

#[derive(Clone, Debug)]
pub struct Binary {
    pub operator: &'static str,
    pub left: NodeId,
    pub right: NodeId,
}

/// A template quasi's `value` (`raw`, `cooked` — cooked is null for an
/// invalid escape in a tagged template).
#[derive(Clone, Debug)]
pub struct Quasi {
    pub raw: String,
    pub cooked: Option<String>,
}

#[derive(Clone, Debug)]
pub enum Kind {
    /// The traversal root (`File.program`).
    File {
        program: NodeId,
    },
    Program {
        interpreter: Option<String>,
        directives: Vec<NodeId>,
        body: Vec<NodeId>,
    },
    Directive {
        value: NodeId,
    },
    DirectiveLiteral {
        raw: String,
    },
    BlockStatement {
        directives: Vec<NodeId>,
        body: Vec<NodeId>,
    },
    StaticBlock {
        body: Vec<NodeId>,
    },
    ExpressionStatement {
        expression: NodeId,
    },
    EmptyStatement,
    DebuggerStatement,
    WithStatement {
        object: NodeId,
        body: NodeId,
    },
    ReturnStatement {
        argument: Option<NodeId>,
    },
    ThrowStatement {
        argument: NodeId,
    },
    LabeledStatement {
        label: NodeId,
        body: NodeId,
    },
    BreakStatement {
        label: Option<NodeId>,
    },
    ContinueStatement {
        label: Option<NodeId>,
    },
    IfStatement {
        test: NodeId,
        consequent: NodeId,
        alternate: Option<NodeId>,
    },
    SwitchStatement {
        discriminant: NodeId,
        cases: Vec<NodeId>,
    },
    SwitchCase {
        test: Option<NodeId>,
        consequent: Vec<NodeId>,
    },
    TryStatement {
        block: NodeId,
        handler: Option<NodeId>,
        finalizer: Option<NodeId>,
    },
    CatchClause {
        param: Option<NodeId>,
        body: NodeId,
    },
    WhileStatement {
        test: NodeId,
        body: NodeId,
    },
    DoWhileStatement {
        body: NodeId,
        test: NodeId,
    },
    ForStatement {
        init: Option<NodeId>,
        test: Option<NodeId>,
        update: Option<NodeId>,
        body: NodeId,
    },
    ForInStatement {
        left: NodeId,
        right: NodeId,
        body: NodeId,
    },
    ForOfStatement {
        is_await: bool,
        left: NodeId,
        right: NodeId,
        body: NodeId,
    },
    FunctionDeclaration(Func),
    FunctionExpression(Func),
    ArrowFunctionExpression(Func),
    VariableDeclaration {
        /// "var" / "let" / "const" / "using" / "await using".
        kind: &'static str,
        declarations: Vec<NodeId>,
    },
    VariableDeclarator {
        id: NodeId,
        init: Option<NodeId>,
    },
    ClassDeclaration(Class),
    ClassExpression(Class),
    ClassBody {
        body: Vec<NodeId>,
    },
    ClassMethod(Method),
    ClassPrivateMethod(Method),
    ClassProperty(Prop),
    ClassPrivateProperty(Prop),
    ClassAccessorProperty(Prop),
    Identifier {
        name: String,
    },
    PrivateName {
        id: NodeId,
    },
    StringLiteral {
        value: String,
        /// `extra.raw` (None: synthesized, printed through jsesc).
        raw: Option<String>,
    },
    NumericLiteral {
        value: f64,
        /// `extra.raw` (None: synthesized, printed as `value + ""`).
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
        quasis: Vec<Quasi>,
        expressions: Vec<NodeId>,
    },
    TaggedTemplateExpression {
        tag: NodeId,
        quasi: NodeId,
    },
    ThisExpression,
    Super,
    Import,
    ArrayExpression {
        elements: Vec<NodeId>,
    },
    ArrayPattern {
        elements: Vec<NodeId>,
    },
    ObjectExpression {
        properties: Vec<NodeId>,
    },
    ObjectPattern {
        properties: Vec<NodeId>,
    },
    ObjectProperty {
        key: NodeId,
        value: NodeId,
        computed: bool,
        shorthand: bool,
    },
    ObjectMethod(Method),
    SpreadElement {
        argument: NodeId,
    },
    RestElement {
        argument: NodeId,
    },
    UnaryExpression {
        operator: &'static str,
        argument: NodeId,
    },
    UpdateExpression {
        operator: &'static str,
        prefix: bool,
        argument: NodeId,
    },
    BinaryExpression(Binary),
    LogicalExpression(Binary),
    AssignmentExpression(Binary),
    AssignmentPattern {
        left: NodeId,
        right: NodeId,
    },
    ConditionalExpression {
        test: NodeId,
        consequent: NodeId,
        alternate: NodeId,
    },
    CallExpression(Call),
    NewExpression(Call),
    OptionalCallExpression(Call),
    MemberExpression(Member),
    OptionalMemberExpression(Member),
    SequenceExpression {
        expressions: Vec<NodeId>,
    },
    YieldExpression {
        argument: Option<NodeId>,
        delegate: bool,
    },
    AwaitExpression {
        argument: NodeId,
    },
    MetaProperty {
        meta: NodeId,
        property: NodeId,
    },
    ImportDeclaration {
        specifiers: Vec<NodeId>,
        source: NodeId,
        attributes: Vec<NodeId>,
        /// `import source x from` / `import defer * as x from`.
        phase: Option<&'static str>,
    },
    ImportSpecifier {
        imported: NodeId,
        local: NodeId,
    },
    ImportDefaultSpecifier {
        local: NodeId,
    },
    ImportNamespaceSpecifier {
        local: NodeId,
    },
    ImportAttribute {
        key: NodeId,
        value: NodeId,
    },
    ExportNamedDeclaration {
        declaration: Option<NodeId>,
        specifiers: Vec<NodeId>,
        source: Option<NodeId>,
        attributes: Vec<NodeId>,
    },
    ExportDefaultDeclaration {
        declaration: NodeId,
    },
    ExportAllDeclaration {
        source: NodeId,
        attributes: Vec<NodeId>,
    },
    ExportSpecifier {
        local: NodeId,
        exported: NodeId,
    },
    ExportNamespaceSpecifier {
        exported: NodeId,
    },
}

/// A child field's name — Babel's property keys, as the traversal and the
/// visitors address them (`path.get("body")`, `path.key === "init"`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Field {
    Program,
    Directives,
    Body,
    Value,
    Expression,
    Object,
    Argument,
    Label,
    Test,
    Consequent,
    Alternate,
    Discriminant,
    Cases,
    Block,
    Handler,
    Finalizer,
    Param,
    Init,
    Update,
    Left,
    Right,
    Id,
    Params,
    Declarations,
    SuperClass,
    Key,
    Quasi,
    Tag,
    Expressions,
    Elements,
    Properties,
    Callee,
    Arguments,
    Property,
    Meta,
    Specifiers,
    Source,
    Attributes,
    Declaration,
    Imported,
    Local,
    Exported,
}

impl Field {
    /// `STATEMENT_OR_BLOCK_KEYS`.
    pub fn is_statement_or_block_key(self) -> bool {
        matches!(self, Field::Consequent | Field::Body | Field::Alternate)
    }
}

/// A field's shape for `validate`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldShape {
    /// Not a field of this node type (no validation).
    Absent,
    Required,
    Optional,
    List,
}

/// One single-node field of a node, writable.
enum SlotMut<'t> {
    Req(&'t mut NodeId),
    Opt(&'t mut Option<NodeId>),
}

/// One field of a node, read.
#[derive(Clone, Copy, Debug)]
pub enum Slot<'t> {
    /// A single-node field (None: absent / null).
    One(Option<NodeId>),
    /// An array field (entries may be [`NodeId::NONE`]).
    Many(&'t [NodeId]),
}

impl Kind {
    /// Babel's `VISITOR_KEYS[type]`, restricted to the keys this AST
    /// carries (type annotations, decorators and the like never occur).
    pub fn visitor_keys(&self) -> &'static [Field] {
        use Field as F;
        match self {
            Kind::File { .. } => &[F::Program],
            Kind::Program { .. } | Kind::BlockStatement { .. } => &[F::Directives, F::Body],
            Kind::Directive { .. } => &[F::Value],
            Kind::StaticBlock { .. } | Kind::ClassBody { .. } => &[F::Body],
            Kind::ExpressionStatement { .. } => &[F::Expression],
            Kind::WithStatement { .. } => &[F::Object, F::Body],
            Kind::ReturnStatement { .. }
            | Kind::ThrowStatement { .. }
            | Kind::SpreadElement { .. }
            | Kind::RestElement { .. }
            | Kind::UnaryExpression { .. }
            | Kind::UpdateExpression { .. }
            | Kind::YieldExpression { .. }
            | Kind::AwaitExpression { .. } => &[F::Argument],
            Kind::LabeledStatement { .. } => &[F::Label, F::Body],
            Kind::BreakStatement { .. } | Kind::ContinueStatement { .. } => &[F::Label],
            Kind::IfStatement { .. } | Kind::ConditionalExpression { .. } => {
                &[F::Test, F::Consequent, F::Alternate]
            }
            Kind::SwitchStatement { .. } => &[F::Discriminant, F::Cases],
            Kind::SwitchCase { .. } => &[F::Test, F::Consequent],
            Kind::TryStatement { .. } => &[F::Block, F::Handler, F::Finalizer],
            Kind::CatchClause { .. } => &[F::Param, F::Body],
            Kind::WhileStatement { .. } => &[F::Test, F::Body],
            Kind::DoWhileStatement { .. } => &[F::Body, F::Test],
            Kind::ForStatement { .. } => &[F::Init, F::Test, F::Update, F::Body],
            Kind::ForInStatement { .. } | Kind::ForOfStatement { .. } => {
                &[F::Left, F::Right, F::Body]
            }
            Kind::FunctionDeclaration(_) | Kind::FunctionExpression(_) => {
                &[F::Id, F::Params, F::Body]
            }
            Kind::ArrowFunctionExpression(_) => &[F::Params, F::Body],
            Kind::VariableDeclaration { .. } => &[F::Declarations],
            Kind::VariableDeclarator { .. } => &[F::Id, F::Init],
            Kind::ClassDeclaration(_) | Kind::ClassExpression(_) => {
                &[F::Id, F::SuperClass, F::Body]
            }
            Kind::ClassMethod(_) | Kind::ClassPrivateMethod(_) | Kind::ObjectMethod(_) => {
                &[F::Key, F::Params, F::Body]
            }
            Kind::ClassProperty(_)
            | Kind::ClassPrivateProperty(_)
            | Kind::ClassAccessorProperty(_) => &[F::Key, F::Value],
            Kind::PrivateName { .. } => &[F::Id],
            Kind::TemplateLiteral { .. } => &[F::Expressions],
            Kind::TaggedTemplateExpression { .. } => &[F::Tag, F::Quasi],
            Kind::ArrayExpression { .. } | Kind::ArrayPattern { .. } => &[F::Elements],
            Kind::ObjectExpression { .. } | Kind::ObjectPattern { .. } => &[F::Properties],
            Kind::ObjectProperty { .. } => &[F::Key, F::Value],
            Kind::BinaryExpression(_)
            | Kind::LogicalExpression(_)
            | Kind::AssignmentExpression(_)
            | Kind::AssignmentPattern { .. } => &[F::Left, F::Right],
            Kind::CallExpression(_) | Kind::NewExpression(_) | Kind::OptionalCallExpression(_) => {
                &[F::Callee, F::Arguments]
            }
            Kind::MemberExpression(_) | Kind::OptionalMemberExpression(_) => {
                &[F::Object, F::Property]
            }
            Kind::SequenceExpression { .. } => &[F::Expressions],
            Kind::MetaProperty { .. } => &[F::Meta, F::Property],
            Kind::ImportDeclaration { .. } => &[F::Specifiers, F::Source, F::Attributes],
            Kind::ImportSpecifier { .. } => &[F::Imported, F::Local],
            Kind::ImportDefaultSpecifier { .. } | Kind::ImportNamespaceSpecifier { .. } => {
                &[F::Local]
            }
            Kind::ImportAttribute { .. } => &[F::Key, F::Value],
            Kind::ExportNamedDeclaration { .. } => {
                &[F::Declaration, F::Specifiers, F::Source, F::Attributes]
            }
            Kind::ExportDefaultDeclaration { .. } => &[F::Declaration],
            Kind::ExportAllDeclaration { .. } => &[F::Source, F::Attributes],
            Kind::ExportSpecifier { .. } => &[F::Local, F::Exported],
            Kind::ExportNamespaceSpecifier { .. } => &[F::Exported],
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
            | Kind::Import => &[],
        }
    }

    /// Read one field (`node[key]`); `Slot::One(None)` for a field this
    /// kind does not have.
    pub fn get(&self, field: Field) -> Slot<'_> {
        match self.single(field) {
            Some(v) => Slot::One(v),
            None => match self.list(field) {
                Some(v) => Slot::Many(v),
                None => Slot::One(None),
            },
        }
    }

    /// A single-node field: Some(value) when this kind has it.
    #[allow(clippy::cognitive_complexity)]
    fn single(&self, field: Field) -> Option<Option<NodeId>> {
        use Field as F;
        // A required field holds NONE only transiently (replaceWithMultiple
        // nulls the slot before it inserts); it reads as null.
        let v = |n: &NodeId| Some(n.opt());
        let o = |n: &Option<NodeId>| Some(n.and_then(NodeId::opt));
        match (self, field) {
            (Kind::File { program }, F::Program) => v(program),
            (Kind::Directive { value }, F::Value) => v(value),
            (Kind::ExpressionStatement { expression }, F::Expression) => v(expression),
            (Kind::WithStatement { object, .. }, F::Object) => v(object),
            (
                Kind::WithStatement { body, .. }
                | Kind::LabeledStatement { body, .. }
                | Kind::WhileStatement { body, .. }
                | Kind::DoWhileStatement { body, .. }
                | Kind::ForStatement { body, .. }
                | Kind::ForInStatement { body, .. }
                | Kind::ForOfStatement { body, .. }
                | Kind::CatchClause { body, .. },
                F::Body,
            ) => v(body),
            (
                Kind::ReturnStatement { argument } | Kind::YieldExpression { argument, .. },
                F::Argument,
            ) => o(argument),
            (
                Kind::ThrowStatement { argument }
                | Kind::SpreadElement { argument }
                | Kind::RestElement { argument }
                | Kind::UnaryExpression { argument, .. }
                | Kind::UpdateExpression { argument, .. }
                | Kind::AwaitExpression { argument },
                F::Argument,
            ) => v(argument),
            (Kind::LabeledStatement { label, .. }, F::Label) => v(label),
            (Kind::BreakStatement { label } | Kind::ContinueStatement { label }, F::Label) => {
                o(label)
            }
            (
                Kind::IfStatement { test, .. }
                | Kind::ConditionalExpression { test, .. }
                | Kind::WhileStatement { test, .. }
                | Kind::DoWhileStatement { test, .. },
                F::Test,
            ) => v(test),
            (Kind::SwitchCase { test, .. } | Kind::ForStatement { test, .. }, F::Test) => o(test),
            (
                Kind::IfStatement { consequent, .. }
                | Kind::ConditionalExpression { consequent, .. },
                F::Consequent,
            ) => v(consequent),
            (Kind::IfStatement { alternate, .. }, F::Alternate) => o(alternate),
            (Kind::ConditionalExpression { alternate, .. }, F::Alternate) => v(alternate),
            (Kind::SwitchStatement { discriminant, .. }, F::Discriminant) => v(discriminant),
            (Kind::TryStatement { block, .. }, F::Block) => v(block),
            (Kind::TryStatement { handler, .. }, F::Handler) => o(handler),
            (Kind::TryStatement { finalizer, .. }, F::Finalizer) => o(finalizer),
            (Kind::CatchClause { param, .. }, F::Param) => o(param),
            (Kind::ForStatement { init, .. }, F::Init) => o(init),
            (Kind::ForStatement { update, .. }, F::Update) => o(update),
            (Kind::VariableDeclarator { init, .. }, F::Init) => o(init),
            (Kind::VariableDeclarator { id, .. }, F::Id) => v(id),
            (
                Kind::ForInStatement { left, .. }
                | Kind::ForOfStatement { left, .. }
                | Kind::AssignmentPattern { left, .. },
                F::Left,
            ) => v(left),
            (
                Kind::ForInStatement { right, .. }
                | Kind::ForOfStatement { right, .. }
                | Kind::AssignmentPattern { right, .. },
                F::Right,
            ) => v(right),
            (
                Kind::BinaryExpression(b)
                | Kind::LogicalExpression(b)
                | Kind::AssignmentExpression(b),
                F::Left,
            ) => v(&b.left),
            (
                Kind::BinaryExpression(b)
                | Kind::LogicalExpression(b)
                | Kind::AssignmentExpression(b),
                F::Right,
            ) => v(&b.right),
            (
                Kind::FunctionDeclaration(f)
                | Kind::FunctionExpression(f)
                | Kind::ArrowFunctionExpression(f),
                F::Id,
            ) => o(&f.id),
            (
                Kind::FunctionDeclaration(f)
                | Kind::FunctionExpression(f)
                | Kind::ArrowFunctionExpression(f),
                F::Body,
            ) => v(&f.body),
            (
                Kind::ClassMethod(m) | Kind::ClassPrivateMethod(m) | Kind::ObjectMethod(m),
                F::Key,
            ) => v(&m.key),
            (
                Kind::ClassMethod(m) | Kind::ClassPrivateMethod(m) | Kind::ObjectMethod(m),
                F::Body,
            ) => v(&m.func.body),
            (Kind::ClassDeclaration(c) | Kind::ClassExpression(c), F::Id) => o(&c.id),
            (Kind::ClassDeclaration(c) | Kind::ClassExpression(c), F::SuperClass) => {
                o(&c.super_class)
            }
            (Kind::ClassDeclaration(c) | Kind::ClassExpression(c), F::Body) => v(&c.body),
            (
                Kind::ClassProperty(p)
                | Kind::ClassPrivateProperty(p)
                | Kind::ClassAccessorProperty(p),
                F::Key,
            ) => v(&p.key),
            (
                Kind::ClassProperty(p)
                | Kind::ClassPrivateProperty(p)
                | Kind::ClassAccessorProperty(p),
                F::Value,
            ) => o(&p.value),
            (Kind::PrivateName { id }, F::Id) => v(id),
            (Kind::TaggedTemplateExpression { tag, .. }, F::Tag) => v(tag),
            (Kind::TaggedTemplateExpression { quasi, .. }, F::Quasi) => v(quasi),
            (Kind::ObjectProperty { key, .. } | Kind::ImportAttribute { key, .. }, F::Key) => {
                v(key)
            }
            (
                Kind::ObjectProperty { value, .. } | Kind::ImportAttribute { value, .. },
                F::Value,
            ) => v(value),
            (
                Kind::CallExpression(c) | Kind::NewExpression(c) | Kind::OptionalCallExpression(c),
                F::Callee,
            ) => v(&c.callee),
            (Kind::MemberExpression(m) | Kind::OptionalMemberExpression(m), F::Object) => {
                v(&m.object)
            }
            (Kind::MemberExpression(m) | Kind::OptionalMemberExpression(m), F::Property) => {
                v(&m.property)
            }
            (Kind::MetaProperty { meta, .. }, F::Meta) => v(meta),
            (Kind::MetaProperty { property, .. }, F::Property) => v(property),
            (
                Kind::ImportDeclaration { source, .. } | Kind::ExportAllDeclaration { source, .. },
                F::Source,
            ) => v(source),
            (Kind::ExportNamedDeclaration { source, .. }, F::Source) => o(source),
            (Kind::ExportNamedDeclaration { declaration, .. }, F::Declaration) => o(declaration),
            (Kind::ExportDefaultDeclaration { declaration }, F::Declaration) => v(declaration),
            (Kind::ImportSpecifier { imported, .. }, F::Imported) => v(imported),
            (
                Kind::ImportSpecifier { local, .. }
                | Kind::ImportDefaultSpecifier { local }
                | Kind::ImportNamespaceSpecifier { local }
                | Kind::ExportSpecifier { local, .. },
                F::Local,
            ) => v(local),
            (
                Kind::ExportSpecifier { exported, .. }
                | Kind::ExportNamespaceSpecifier { exported },
                F::Exported,
            ) => v(exported),
            _ => None,
        }
    }

    /// An array field: Some(entries) when this kind has it.
    fn list(&self, field: Field) -> Option<&[NodeId]> {
        use Field as F;
        Some(match (self, field) {
            (
                Kind::Program { directives, .. } | Kind::BlockStatement { directives, .. },
                F::Directives,
            ) => directives,
            (
                Kind::Program { body, .. }
                | Kind::BlockStatement { body, .. }
                | Kind::StaticBlock { body }
                | Kind::ClassBody { body },
                F::Body,
            ) => body,
            (Kind::SwitchStatement { cases, .. }, F::Cases) => cases,
            (Kind::SwitchCase { consequent, .. }, F::Consequent) => consequent,
            (
                Kind::FunctionDeclaration(f)
                | Kind::FunctionExpression(f)
                | Kind::ArrowFunctionExpression(f),
                F::Params,
            ) => &f.params,
            (
                Kind::ClassMethod(m) | Kind::ClassPrivateMethod(m) | Kind::ObjectMethod(m),
                F::Params,
            ) => &m.func.params,
            (Kind::VariableDeclaration { declarations, .. }, F::Declarations) => declarations,
            (
                Kind::TemplateLiteral { expressions, .. }
                | Kind::SequenceExpression { expressions },
                F::Expressions,
            ) => expressions,
            (Kind::ArrayExpression { elements } | Kind::ArrayPattern { elements }, F::Elements) => {
                elements
            }
            (
                Kind::ObjectExpression { properties } | Kind::ObjectPattern { properties },
                F::Properties,
            ) => properties,
            (
                Kind::CallExpression(c) | Kind::NewExpression(c) | Kind::OptionalCallExpression(c),
                F::Arguments,
            ) => &c.arguments,
            (
                Kind::ImportDeclaration { specifiers, .. }
                | Kind::ExportNamedDeclaration { specifiers, .. },
                F::Specifiers,
            ) => specifiers,
            (
                Kind::ImportDeclaration { attributes, .. }
                | Kind::ExportNamedDeclaration { attributes, .. }
                | Kind::ExportAllDeclaration { attributes, .. },
                F::Attributes,
            ) => attributes,
            _ => return None,
        })
    }

    /// A mutable array field (the traversal's `container`).
    pub fn list_mut(&mut self, field: Field) -> Option<&mut Vec<NodeId>> {
        use Field as F;
        Some(match (self, field) {
            (
                Kind::Program { directives, .. } | Kind::BlockStatement { directives, .. },
                F::Directives,
            ) => directives,
            (
                Kind::Program { body, .. }
                | Kind::BlockStatement { body, .. }
                | Kind::StaticBlock { body }
                | Kind::ClassBody { body },
                F::Body,
            ) => body,
            (Kind::SwitchStatement { cases, .. }, F::Cases) => cases,
            (Kind::SwitchCase { consequent, .. }, F::Consequent) => consequent,
            (
                Kind::FunctionDeclaration(f)
                | Kind::FunctionExpression(f)
                | Kind::ArrowFunctionExpression(f),
                F::Params,
            ) => &mut f.params,
            (
                Kind::ClassMethod(m) | Kind::ClassPrivateMethod(m) | Kind::ObjectMethod(m),
                F::Params,
            ) => &mut m.func.params,
            (Kind::VariableDeclaration { declarations, .. }, F::Declarations) => declarations,
            (
                Kind::TemplateLiteral { expressions, .. }
                | Kind::SequenceExpression { expressions },
                F::Expressions,
            ) => expressions,
            (Kind::ArrayExpression { elements } | Kind::ArrayPattern { elements }, F::Elements) => {
                elements
            }
            (
                Kind::ObjectExpression { properties } | Kind::ObjectPattern { properties },
                F::Properties,
            ) => properties,
            (
                Kind::CallExpression(c) | Kind::NewExpression(c) | Kind::OptionalCallExpression(c),
                F::Arguments,
            ) => &mut c.arguments,
            (
                Kind::ImportDeclaration { specifiers, .. }
                | Kind::ExportNamedDeclaration { specifiers, .. },
                F::Specifiers,
            ) => specifiers,
            (
                Kind::ImportDeclaration { attributes, .. }
                | Kind::ExportNamedDeclaration { attributes, .. }
                | Kind::ExportAllDeclaration { attributes, .. },
                F::Attributes,
            ) => attributes,
            _ => return None,
        })
    }

    /// A mutable single-node field: Some(slot) when this kind has it.
    #[allow(clippy::cognitive_complexity)]
    fn single_mut(&mut self, field: Field) -> Option<SlotMut<'_>> {
        use Field as F;
        Some(match (self, field) {
            (Kind::File { program }, F::Program) => SlotMut::Req(program),
            (Kind::Directive { value }, F::Value) => SlotMut::Req(value),
            (Kind::ExpressionStatement { expression }, F::Expression) => SlotMut::Req(expression),
            (Kind::WithStatement { object, .. }, F::Object) => SlotMut::Req(object),
            (
                Kind::WithStatement { body, .. }
                | Kind::LabeledStatement { body, .. }
                | Kind::WhileStatement { body, .. }
                | Kind::DoWhileStatement { body, .. }
                | Kind::ForStatement { body, .. }
                | Kind::ForInStatement { body, .. }
                | Kind::ForOfStatement { body, .. }
                | Kind::CatchClause { body, .. },
                F::Body,
            ) => SlotMut::Req(body),
            (
                Kind::ReturnStatement { argument } | Kind::YieldExpression { argument, .. },
                F::Argument,
            ) => SlotMut::Opt(argument),
            (
                Kind::ThrowStatement { argument }
                | Kind::SpreadElement { argument }
                | Kind::RestElement { argument }
                | Kind::UnaryExpression { argument, .. }
                | Kind::UpdateExpression { argument, .. }
                | Kind::AwaitExpression { argument },
                F::Argument,
            ) => SlotMut::Req(argument),
            (Kind::LabeledStatement { label, .. }, F::Label) => SlotMut::Req(label),
            (Kind::BreakStatement { label } | Kind::ContinueStatement { label }, F::Label) => {
                SlotMut::Opt(label)
            }
            (
                Kind::IfStatement { test, .. }
                | Kind::ConditionalExpression { test, .. }
                | Kind::WhileStatement { test, .. }
                | Kind::DoWhileStatement { test, .. },
                F::Test,
            ) => SlotMut::Req(test),
            (Kind::SwitchCase { test, .. } | Kind::ForStatement { test, .. }, F::Test) => {
                SlotMut::Opt(test)
            }
            (
                Kind::IfStatement { consequent, .. }
                | Kind::ConditionalExpression { consequent, .. },
                F::Consequent,
            ) => SlotMut::Req(consequent),
            (Kind::IfStatement { alternate, .. }, F::Alternate) => SlotMut::Opt(alternate),
            (Kind::ConditionalExpression { alternate, .. }, F::Alternate) => {
                SlotMut::Req(alternate)
            }
            (Kind::SwitchStatement { discriminant, .. }, F::Discriminant) => {
                SlotMut::Req(discriminant)
            }
            (Kind::TryStatement { block, .. }, F::Block) => SlotMut::Req(block),
            (Kind::TryStatement { handler, .. }, F::Handler) => SlotMut::Opt(handler),
            (Kind::TryStatement { finalizer, .. }, F::Finalizer) => SlotMut::Opt(finalizer),
            (Kind::CatchClause { param, .. }, F::Param) => SlotMut::Opt(param),
            (Kind::ForStatement { init, .. }, F::Init) => SlotMut::Opt(init),
            (Kind::ForStatement { update, .. }, F::Update) => SlotMut::Opt(update),
            (Kind::VariableDeclarator { init, .. }, F::Init) => SlotMut::Opt(init),
            (Kind::VariableDeclarator { id, .. }, F::Id) => SlotMut::Req(id),
            (
                Kind::ForInStatement { left, .. }
                | Kind::ForOfStatement { left, .. }
                | Kind::AssignmentPattern { left, .. },
                F::Left,
            ) => SlotMut::Req(left),
            (
                Kind::ForInStatement { right, .. }
                | Kind::ForOfStatement { right, .. }
                | Kind::AssignmentPattern { right, .. },
                F::Right,
            ) => SlotMut::Req(right),
            (
                Kind::BinaryExpression(b)
                | Kind::LogicalExpression(b)
                | Kind::AssignmentExpression(b),
                F::Left,
            ) => SlotMut::Req(&mut b.left),
            (
                Kind::BinaryExpression(b)
                | Kind::LogicalExpression(b)
                | Kind::AssignmentExpression(b),
                F::Right,
            ) => SlotMut::Req(&mut b.right),
            (
                Kind::FunctionDeclaration(f)
                | Kind::FunctionExpression(f)
                | Kind::ArrowFunctionExpression(f),
                F::Id,
            ) => SlotMut::Opt(&mut f.id),
            (
                Kind::FunctionDeclaration(f)
                | Kind::FunctionExpression(f)
                | Kind::ArrowFunctionExpression(f),
                F::Body,
            ) => SlotMut::Req(&mut f.body),
            (
                Kind::ClassMethod(m) | Kind::ClassPrivateMethod(m) | Kind::ObjectMethod(m),
                F::Key,
            ) => SlotMut::Req(&mut m.key),
            (
                Kind::ClassMethod(m) | Kind::ClassPrivateMethod(m) | Kind::ObjectMethod(m),
                F::Body,
            ) => SlotMut::Req(&mut m.func.body),
            (Kind::ClassDeclaration(c) | Kind::ClassExpression(c), F::Id) => {
                SlotMut::Opt(&mut c.id)
            }
            (Kind::ClassDeclaration(c) | Kind::ClassExpression(c), F::SuperClass) => {
                SlotMut::Opt(&mut c.super_class)
            }
            (Kind::ClassDeclaration(c) | Kind::ClassExpression(c), F::Body) => {
                SlotMut::Req(&mut c.body)
            }
            (
                Kind::ClassProperty(p)
                | Kind::ClassPrivateProperty(p)
                | Kind::ClassAccessorProperty(p),
                F::Key,
            ) => SlotMut::Req(&mut p.key),
            (
                Kind::ClassProperty(p)
                | Kind::ClassPrivateProperty(p)
                | Kind::ClassAccessorProperty(p),
                F::Value,
            ) => SlotMut::Opt(&mut p.value),
            (Kind::PrivateName { id }, F::Id) => SlotMut::Req(id),
            (Kind::TaggedTemplateExpression { tag, .. }, F::Tag) => SlotMut::Req(tag),
            (Kind::TaggedTemplateExpression { quasi, .. }, F::Quasi) => SlotMut::Req(quasi),
            (Kind::ObjectProperty { key, .. } | Kind::ImportAttribute { key, .. }, F::Key) => {
                SlotMut::Req(key)
            }
            (
                Kind::ObjectProperty { value, .. } | Kind::ImportAttribute { value, .. },
                F::Value,
            ) => SlotMut::Req(value),
            (
                Kind::CallExpression(c) | Kind::NewExpression(c) | Kind::OptionalCallExpression(c),
                F::Callee,
            ) => SlotMut::Req(&mut c.callee),
            (Kind::MemberExpression(m) | Kind::OptionalMemberExpression(m), F::Object) => {
                SlotMut::Req(&mut m.object)
            }
            (Kind::MemberExpression(m) | Kind::OptionalMemberExpression(m), F::Property) => {
                SlotMut::Req(&mut m.property)
            }
            (Kind::MetaProperty { meta, .. }, F::Meta) => SlotMut::Req(meta),
            (Kind::MetaProperty { property, .. }, F::Property) => SlotMut::Req(property),
            (
                Kind::ImportDeclaration { source, .. } | Kind::ExportAllDeclaration { source, .. },
                F::Source,
            ) => SlotMut::Req(source),
            (Kind::ExportNamedDeclaration { source, .. }, F::Source) => SlotMut::Opt(source),
            (Kind::ExportNamedDeclaration { declaration, .. }, F::Declaration) => {
                SlotMut::Opt(declaration)
            }
            (Kind::ExportDefaultDeclaration { declaration }, F::Declaration) => {
                SlotMut::Req(declaration)
            }
            (Kind::ImportSpecifier { imported, .. }, F::Imported) => SlotMut::Req(imported),
            (
                Kind::ImportSpecifier { local, .. }
                | Kind::ImportDefaultSpecifier { local }
                | Kind::ImportNamespaceSpecifier { local }
                | Kind::ExportSpecifier { local, .. },
                F::Local,
            ) => SlotMut::Req(local),
            (
                Kind::ExportSpecifier { exported, .. }
                | Kind::ExportNamespaceSpecifier { exported },
                F::Exported,
            ) => SlotMut::Req(exported),
            _ => return None,
        })
    }

    /// The shape of a field on this node type (Babel's `NODE_FIELDS`
    /// entry, reduced to what `validate` checks on a null / node write).
    pub fn field_shape(&mut self, field: Field) -> FieldShape {
        if self.list_mut(field).is_some() {
            return FieldShape::List;
        }
        match self.single_mut(field) {
            Some(SlotMut::Req(_)) => FieldShape::Required,
            Some(SlotMut::Opt(_)) => FieldShape::Optional,
            None => FieldShape::Absent,
        }
    }

    /// Write a single-node field (`node[key] = value`). Err when this kind
    /// has no such field. A required field written with None holds NONE
    /// (the transient null of `replaceWithMultiple`).
    pub fn set(&mut self, field: Field, value: Option<NodeId>) -> Result<(), String> {
        let name = self.type_name();
        match self.single_mut(field) {
            Some(SlotMut::Req(slot)) => *slot = value.unwrap_or(NodeId::NONE),
            Some(SlotMut::Opt(slot)) => *slot = value,
            None => return Err(format!("{name} has no field {field:?}")),
        }
        Ok(())
    }

    /// Babel's `node.type`.
    pub fn type_name(&self) -> &'static str {
        match self {
            Kind::File { .. } => "File",
            Kind::Program { .. } => "Program",
            Kind::Directive { .. } => "Directive",
            Kind::DirectiveLiteral { .. } => "DirectiveLiteral",
            Kind::BlockStatement { .. } => "BlockStatement",
            Kind::StaticBlock { .. } => "StaticBlock",
            Kind::ExpressionStatement { .. } => "ExpressionStatement",
            Kind::EmptyStatement => "EmptyStatement",
            Kind::DebuggerStatement => "DebuggerStatement",
            Kind::WithStatement { .. } => "WithStatement",
            Kind::ReturnStatement { .. } => "ReturnStatement",
            Kind::ThrowStatement { .. } => "ThrowStatement",
            Kind::LabeledStatement { .. } => "LabeledStatement",
            Kind::BreakStatement { .. } => "BreakStatement",
            Kind::ContinueStatement { .. } => "ContinueStatement",
            Kind::IfStatement { .. } => "IfStatement",
            Kind::SwitchStatement { .. } => "SwitchStatement",
            Kind::SwitchCase { .. } => "SwitchCase",
            Kind::TryStatement { .. } => "TryStatement",
            Kind::CatchClause { .. } => "CatchClause",
            Kind::WhileStatement { .. } => "WhileStatement",
            Kind::DoWhileStatement { .. } => "DoWhileStatement",
            Kind::ForStatement { .. } => "ForStatement",
            Kind::ForInStatement { .. } => "ForInStatement",
            Kind::ForOfStatement { .. } => "ForOfStatement",
            Kind::FunctionDeclaration(_) => "FunctionDeclaration",
            Kind::FunctionExpression(_) => "FunctionExpression",
            Kind::ArrowFunctionExpression(_) => "ArrowFunctionExpression",
            Kind::VariableDeclaration { .. } => "VariableDeclaration",
            Kind::VariableDeclarator { .. } => "VariableDeclarator",
            Kind::ClassDeclaration(_) => "ClassDeclaration",
            Kind::ClassExpression(_) => "ClassExpression",
            Kind::ClassBody { .. } => "ClassBody",
            Kind::ClassMethod(_) => "ClassMethod",
            Kind::ClassPrivateMethod(_) => "ClassPrivateMethod",
            Kind::ClassProperty(_) => "ClassProperty",
            Kind::ClassPrivateProperty(_) => "ClassPrivateProperty",
            Kind::ClassAccessorProperty(_) => "ClassAccessorProperty",
            Kind::Identifier { .. } => "Identifier",
            Kind::PrivateName { .. } => "PrivateName",
            Kind::StringLiteral { .. } => "StringLiteral",
            Kind::NumericLiteral { .. } => "NumericLiteral",
            Kind::BigIntLiteral { .. } => "BigIntLiteral",
            Kind::BooleanLiteral { .. } => "BooleanLiteral",
            Kind::NullLiteral => "NullLiteral",
            Kind::RegExpLiteral { .. } => "RegExpLiteral",
            Kind::TemplateLiteral { .. } => "TemplateLiteral",
            Kind::TaggedTemplateExpression { .. } => "TaggedTemplateExpression",
            Kind::ThisExpression => "ThisExpression",
            Kind::Super => "Super",
            Kind::Import => "Import",
            Kind::ArrayExpression { .. } => "ArrayExpression",
            Kind::ArrayPattern { .. } => "ArrayPattern",
            Kind::ObjectExpression { .. } => "ObjectExpression",
            Kind::ObjectPattern { .. } => "ObjectPattern",
            Kind::ObjectProperty { .. } => "ObjectProperty",
            Kind::ObjectMethod(_) => "ObjectMethod",
            Kind::SpreadElement { .. } => "SpreadElement",
            Kind::RestElement { .. } => "RestElement",
            Kind::UnaryExpression { .. } => "UnaryExpression",
            Kind::UpdateExpression { .. } => "UpdateExpression",
            Kind::BinaryExpression(_) => "BinaryExpression",
            Kind::LogicalExpression(_) => "LogicalExpression",
            Kind::AssignmentExpression(_) => "AssignmentExpression",
            Kind::AssignmentPattern { .. } => "AssignmentPattern",
            Kind::ConditionalExpression { .. } => "ConditionalExpression",
            Kind::CallExpression(_) => "CallExpression",
            Kind::NewExpression(_) => "NewExpression",
            Kind::OptionalCallExpression(_) => "OptionalCallExpression",
            Kind::MemberExpression(_) => "MemberExpression",
            Kind::OptionalMemberExpression(_) => "OptionalMemberExpression",
            Kind::SequenceExpression { .. } => "SequenceExpression",
            Kind::YieldExpression { .. } => "YieldExpression",
            Kind::AwaitExpression { .. } => "AwaitExpression",
            Kind::MetaProperty { .. } => "MetaProperty",
            Kind::ImportDeclaration { .. } => "ImportDeclaration",
            Kind::ImportSpecifier { .. } => "ImportSpecifier",
            Kind::ImportDefaultSpecifier { .. } => "ImportDefaultSpecifier",
            Kind::ImportNamespaceSpecifier { .. } => "ImportNamespaceSpecifier",
            Kind::ImportAttribute { .. } => "ImportAttribute",
            Kind::ExportNamedDeclaration { .. } => "ExportNamedDeclaration",
            Kind::ExportDefaultDeclaration { .. } => "ExportDefaultDeclaration",
            Kind::ExportAllDeclaration { .. } => "ExportAllDeclaration",
            Kind::ExportSpecifier { .. } => "ExportSpecifier",
            Kind::ExportNamespaceSpecifier { .. } => "ExportNamespaceSpecifier",
        }
    }

    /// Babel's `isStatement` (the `Statement` alias: statements and
    /// declarations, module declarations included).
    pub fn is_statement(&self) -> bool {
        matches!(
            self,
            Kind::BlockStatement { .. }
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
                | Kind::ImportDeclaration { .. }
                | Kind::ExportNamedDeclaration { .. }
                | Kind::ExportDefaultDeclaration { .. }
                | Kind::ExportAllDeclaration { .. }
        )
    }

    /// Babel's `isExpression` (the `Expression` alias).
    pub fn is_expression(&self) -> bool {
        matches!(
            self,
            Kind::FunctionExpression(_)
                | Kind::ArrowFunctionExpression(_)
                | Kind::ClassExpression(_)
                | Kind::Identifier { .. }
                | Kind::StringLiteral { .. }
                | Kind::NumericLiteral { .. }
                | Kind::BigIntLiteral { .. }
                | Kind::BooleanLiteral { .. }
                | Kind::NullLiteral
                | Kind::RegExpLiteral { .. }
                | Kind::TemplateLiteral { .. }
                | Kind::TaggedTemplateExpression { .. }
                | Kind::ThisExpression
                | Kind::Super
                | Kind::Import
                | Kind::ArrayExpression { .. }
                | Kind::ObjectExpression { .. }
                | Kind::UnaryExpression { .. }
                | Kind::UpdateExpression { .. }
                | Kind::BinaryExpression(_)
                | Kind::LogicalExpression(_)
                | Kind::AssignmentExpression(_)
                | Kind::ConditionalExpression { .. }
                | Kind::CallExpression(_)
                | Kind::NewExpression(_)
                | Kind::OptionalCallExpression(_)
                | Kind::MemberExpression(_)
                | Kind::OptionalMemberExpression(_)
                | Kind::SequenceExpression { .. }
                | Kind::YieldExpression { .. }
                | Kind::AwaitExpression { .. }
                | Kind::MetaProperty { .. }
        )
    }

    /// Babel's `isLiteral` (template literals included).
    pub fn is_literal(&self) -> bool {
        matches!(
            self,
            Kind::StringLiteral { .. }
                | Kind::NumericLiteral { .. }
                | Kind::BigIntLiteral { .. }
                | Kind::BooleanLiteral { .. }
                | Kind::NullLiteral
                | Kind::RegExpLiteral { .. }
                | Kind::TemplateLiteral { .. }
        )
    }

    /// Babel's `isFunction` alias.
    pub fn is_function(&self) -> bool {
        matches!(
            self,
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
        match self {
            Kind::FunctionDeclaration(f)
            | Kind::FunctionExpression(f)
            | Kind::ArrowFunctionExpression(f) => Some(f),
            Kind::ObjectMethod(m) | Kind::ClassMethod(m) | Kind::ClassPrivateMethod(m) => {
                Some(&m.func)
            }
            _ => None,
        }
    }

    pub fn identifier_name(&self) -> Option<&str> {
        match self {
            Kind::Identifier { name } => Some(name),
            _ => None,
        }
    }
}

/// The arena.
#[derive(Clone, Debug, Default)]
pub struct Tree {
    pub nodes: Vec<Node>,
    pub comments: Vec<Comment>,
    /// Each parenthesized node's parentheses span (`(` … `)`), innermost
    /// first — the parser's `takeSurroundingComments` positions.
    pub parens: Vec<(NodeId, u32, u32)>,
}

impl Tree {
    pub fn new() -> Tree {
        Tree::default()
    }

    /// Add a node; its id.
    pub fn alloc(&mut self, node: Node) -> NodeId {
        let id = NodeId(self.nodes.len() as u32);
        self.nodes.push(node);
        id
    }

    /// A synthesized node (no `loc`, no span), as a Babel builder makes.
    pub fn synth(&mut self, kind: Kind) -> NodeId {
        self.alloc(Node::synth(kind))
    }

    pub fn ident(&mut self, name: &str) -> NodeId {
        self.synth(Kind::Identifier {
            name: name.to_string(),
        })
    }

    pub fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id.0 as usize]
    }

    pub fn node_mut(&mut self, id: NodeId) -> &mut Node {
        &mut self.nodes[id.0 as usize]
    }

    pub fn kind(&self, id: NodeId) -> &Kind {
        &self.node(id).kind
    }

    pub fn kind_mut(&mut self, id: NodeId) -> &mut Kind {
        &mut self.node_mut(id).kind
    }

    /// `isLastChild(parent, child)` (generator node/index.ts): from the last
    /// visitor key backwards, the first PRESENT field decides — an array
    /// field by its last non-null element (an empty array answers false).
    pub fn is_last_child(&self, parent: NodeId, child: NodeId) -> bool {
        let kind = self.kind(parent);
        for key in kind.visitor_keys().iter().rev() {
            match kind.get(*key) {
                Slot::One(None) => continue,
                Slot::One(Some(n)) => return n == child,
                Slot::Many(v) => {
                    return v.iter().rev().find(|n| !n.is_none()) == Some(&child);
                }
            }
        }
        false
    }

    /// A node's children in `VISITOR_KEYS` order (holes skipped).
    pub fn children(&self, id: NodeId) -> Vec<NodeId> {
        let kind = self.kind(id);
        let mut out = Vec::new();
        for key in kind.visitor_keys() {
            match kind.get(*key) {
                Slot::One(Some(n)) => out.push(n),
                Slot::One(None) => {}
                Slot::Many(v) => out.extend(v.iter().copied().filter(|n| !n.is_none())),
            }
        }
        out
    }

    /// The comments attached to `id` (empty when none).
    pub fn comments_of(&self, id: NodeId) -> Option<&NodeComments> {
        self.node(id).comments.as_deref()
    }
}

impl Node {
    pub fn new(kind: Kind, loc: Option<Loc>) -> Node {
        Node {
            kind,
            loc,
            span: None,
            parenthesized: false,
            compact: false,
            comments: None,
        }
    }

    /// A synthesized node (no `loc`), as a Babel template or builder makes.
    pub fn synth(kind: Kind) -> Node {
        Node::new(kind, None)
    }
}
