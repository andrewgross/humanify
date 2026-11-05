//! `@babel/traverse` 7.29.7's engine over the arena — `context.js`
//! (TraversalContext: visitQueue, the priority queue), `traverse-node.js`,
//! `path/context.js` (visit, call, resync, push/popContext, requeue),
//! `path/index.js` (NodePath.get and its per-parent cache),
//! `path/replacement.js`, `path/modification.js`, `path/removal.js` (with
//! the removal hooks) and `path/comments.js` (shareCommentsWithSiblings).
//!
//! It is ported state for state, because the beautify's OUTPUT depends on
//! the order in which nodes are visited and re-visited, and that order is
//! decided by these mechanisms, not by the visitors:
//!
//! - a replaced node is REQUEUED: its path goes onto the priority queue of
//!   every context it is on, and is visited again (with a fresh `visited`
//!   set) right after the path currently being visited finishes;
//! - an inserted sibling is appended to the END of its container's queue
//!   (visited after every original sibling);
//! - a visitor fn that replaces or removes its node stops the node's
//!   remaining fns AND its children (they are visited through the requeue);
//! - a path whose parent statement was replaced keeps traversing the
//!   detached node's children, through its stale parent — so a
//!   `a && (b, c)` LogicalExpression visits its SequenceExpression once
//!   with a LogicalExpression parent and again, after the requeue, inside
//!   the new IfStatement;
//! - paths are cached per (parent node, node) and carry `contexts`,
//!   `key` and flags across traversals (`updateSiblingKeys` keeps the keys
//!   of cached siblings right as the container grows and shrinks).
//!
//! The scope model is reduced to the one question the visitors ask
//! (`hasBinding("undefined", { noGlobals: true })`, see `super::scope`).

use std::collections::{HashMap, HashSet};
use std::hash::{BuildHasherDefault, Hasher};

use super::ast::{Field, FieldShape, Kind, Node, NodeComments, NodeId, Slot, Tree};

/// A path's index.
pub type PathId = u32;
type CtxId = u32;
type QueueId = u32;

type R<T> = Result<T, String>;

/// FxHash for the u32-keyed maps (no dependency; iteration order is never
/// read).
#[derive(Default)]
struct FxHasher(u64);

impl Hasher for FxHasher {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        for b in bytes {
            self.write_u64(u64::from(*b));
        }
    }
    fn write_u32(&mut self, i: u32) {
        self.write_u64(u64::from(i));
    }
    fn write_u64(&mut self, i: u64) {
        self.0 = (self.0.rotate_left(5) ^ i).wrapping_mul(0x51_7c_c1_b7_27_22_0a_95);
    }
    fn write_u8(&mut self, i: u8) {
        self.write_u64(u64::from(i));
    }
    fn write_usize(&mut self, i: usize) {
        self.write_u64(i as u64);
    }
}

type Fx = BuildHasherDefault<FxHasher>;

/// A path's `key`: an array index, a field name, or null.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Index(usize),
    Field(Field),
    Null,
}

/// A path's `container`: the parent node (a single-node field) or one of
/// its array fields. None: Babel's `container = null`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Container {
    Node(NodeId),
    List(NodeId, Field),
}

const REMOVED: u8 = 1;
const SHOULD_STOP: u8 = 2;
const SHOULD_SKIP: u8 = 4;

#[derive(Clone, Debug)]
struct PathData {
    parent_path: Option<PathId>,
    parent: NodeId,
    container: Option<Container>,
    list_key: Option<Field>,
    key: Key,
    node: Option<NodeId>,
    /// The node `this.type` was read from (it survives `node = null`).
    type_node: Option<NodeId>,
    contexts: Vec<CtxId>,
    context: Option<CtxId>,
    flags: u8,
    /// The parent node this path object was created (and cached) under.
    created_under: NodeId,
}

#[derive(Clone, Debug)]
struct CtxData {
    queue: Option<QueueId>,
    priority: QueueId,
    parent_path: Option<PathId>,
    /// A scope crawl's traversal (the collector visitor, no stage-6 fns).
    crawl: bool,
}

/// Which path object owns a scope node's cached `Scope` (`scopeCache`):
/// the path the initial program crawl created under the node's original
/// parent, or a later path that made a new Scope.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ScopeOwner {
    Original(NodeId),
    Path(PathId),
}

/// A visitor fn (`fn(path)`), called with the engine.
pub type VisitFn = fn(&mut Engine<'_>, PathId) -> R<()>;

/// The merged visitor: per node type, the enter fns and the exit fns in
/// plugin order (`traverse.visitors.merge`).
pub trait Visitor {
    /// `opts[type]` exists (a visitor for this type, enter or exit).
    fn has(&self, kind: &Kind) -> bool;
    fn enter(&self, kind: &Kind) -> &[VisitFn];
    fn exit(&self, kind: &Kind) -> &[VisitFn];
}

/// Per-parent path cache (`cache.path`: parent node → Map(node → path)),
/// plus the order paths were created in (the Map's values, for
/// `updateSiblingKeys`).
#[derive(Default)]
struct ParentPaths {
    map: HashMap<NodeId, PathId, Fx>,
    all: Vec<PathId>,
}

pub struct Engine<'t> {
    pub tree: &'t mut Tree,
    visitor: &'t dyn Visitor,
    paths: Vec<PathData>,
    cache: HashMap<NodeId, ParentPaths, Fx>,
    ctxs: Vec<CtxData>,
    queues: Vec<Vec<PathId>>,
    /// Scope nodes whose Babel scope has an own binding `undefined`.
    undefined_scopes: &'t HashSet<NodeId>,
    /// Plant (gate red runs): requeue onto the sibling queue's end.
    requeue_deferred: bool,
    /// Plant: never requeue.
    no_requeue: bool,
    /// Plant: never crawl a new scope.
    no_crawl: bool,
    /// `scopeCache`: scope node → the path its current Scope was made for.
    scope_owner: HashMap<NodeId, ScopeOwner, Fx>,
    /// The scope paths whose crawl is running (`scope.crawling`).
    crawling: HashSet<PathId, Fx>,
}

impl<'t> Engine<'t> {
    pub fn new(
        tree: &'t mut Tree,
        visitor: &'t dyn Visitor,
        undefined_scopes: &'t HashSet<NodeId>,
    ) -> Engine<'t> {
        Engine {
            tree,
            visitor,
            paths: Vec::new(),
            cache: HashMap::default(),
            ctxs: Vec::new(),
            queues: Vec::new(),
            undefined_scopes,
            requeue_deferred: false,
            no_requeue: false,
            no_crawl: false,
            scope_owner: HashMap::default(),
            crawling: HashSet::default(),
        }
    }

    /// Apply a planted perturbation (gate red runs only).
    pub fn plant(&mut self, plant: Option<super::Plant>) {
        use super::Plant;
        match plant {
            Some(Plant::RequeueDeferred) => self.requeue_deferred = true,
            Some(Plant::NoRequeue) => self.no_requeue = true,
            Some(Plant::NoCrawl) => self.no_crawl = true,
            _ => {}
        }
    }

    /// `traverse(file, visitor)`: the root's children, then done.
    ///
    /// `@babel/core`'s File constructor has already crawled the program
    /// (`file.scope`), creating a path and an inited Scope for every scope
    /// node under its ORIGINAL parent; that crawl is modeled by recording
    /// those owners (the paths themselves are made lazily, and are the same
    /// objects: one per parent node and node).
    pub fn traverse(&mut self, root: NodeId) -> R<()> {
        let mut stack = vec![root];
        while let Some(n) = stack.pop() {
            for c in self.tree.children(n) {
                if is_scope(self.kind(c), self.kind(n)) {
                    self.scope_owner.insert(c, ScopeOwner::Original(n));
                }
                stack.push(c);
            }
        }
        if let Kind::File { program } = *self.kind(root) {
            self.scope_owner.insert(program, ScopeOwner::Original(root));
        }
        self.traverse_node(root, None, false)?;
        Ok(())
    }

    // -- node reads ------------------------------------------------------------

    pub fn kind(&self, id: NodeId) -> &Kind {
        self.tree.kind(id)
    }

    /// `path.node` (None: removed / null).
    pub fn node(&self, p: PathId) -> Option<NodeId> {
        self.paths[p as usize].node
    }

    pub fn parent_path(&self, p: PathId) -> Option<PathId> {
        self.paths[p as usize].parent_path
    }

    pub fn key(&self, p: PathId) -> Key {
        self.paths[p as usize].key
    }

    pub fn list_key(&self, p: PathId) -> Option<Field> {
        self.paths[p as usize].list_key
    }

    /// `path.isX()` for a node-kind predicate (false for a null node).
    pub fn path_is(&self, p: PathId, pred: fn(&Kind) -> bool) -> bool {
        self.node(p).is_some_and(|n| pred(self.kind(n)))
    }

    /// `path.isNodeType(t)` — reads `path.type`, which survives removal.
    fn type_is(&self, p: PathId, pred: fn(&Kind) -> bool) -> bool {
        self.paths[p as usize]
            .type_node
            .is_some_and(|n| pred(self.kind(n)))
    }

    fn flags(&self, p: PathId) -> u8 {
        self.paths[p as usize].flags
    }

    fn removed(&self, p: PathId) -> bool {
        self.flags(p) & REMOVED != 0
    }

    fn should_skip(&self, p: PathId) -> bool {
        self.flags(p) & SHOULD_SKIP != 0
    }

    fn should_stop(&self, p: PathId) -> bool {
        self.flags(p) & SHOULD_STOP != 0
    }

    fn set_should_stop(&mut self, p: PathId, v: bool) {
        let f = &mut self.paths[p as usize].flags;
        if v {
            *f |= SHOULD_STOP;
        } else {
            *f &= !SHOULD_STOP;
        }
    }

    // -- containers --------------------------------------------------------------

    /// `container[key]` (None for null / undefined / out of range).
    fn container_get(&self, c: Option<Container>, key: Key) -> Option<NodeId> {
        match (c?, key) {
            (Container::Node(n), Key::Field(f)) => match self.kind(n).get(f) {
                Slot::One(v) => v,
                Slot::Many(_) => None,
            },
            (Container::List(n, f), Key::Index(i)) => match self.kind(n).get(f) {
                Slot::Many(v) => v.get(i).copied().and_then(NodeId::opt),
                Slot::One(_) => None,
            },
            _ => None,
        }
    }

    fn list(&self, c: Container) -> &[NodeId] {
        match c {
            Container::List(n, f) => match self.kind(n).get(f) {
                Slot::Many(v) => v,
                Slot::One(_) => &[],
            },
            Container::Node(_) => &[],
        }
    }

    fn list_mut(&mut self, c: Container) -> R<&mut Vec<NodeId>> {
        match c {
            Container::List(n, f) => self
                .tree
                .kind_mut(n)
                .list_mut(f)
                .ok_or_else(|| format!("no list field {f:?}")),
            Container::Node(_) => Err("not a list container".into()),
        }
    }

    // -- the path cache (path/index.js NodePath.get) -----------------------------

    fn cached_paths(&mut self, parent: NodeId) -> &mut ParentPaths {
        self.cache.entry(parent).or_default()
    }

    /// `NodePath.get({ parentPath, parent, container, listKey, key })`.
    fn path_get(
        &mut self,
        parent_path: Option<PathId>,
        parent: NodeId,
        container: Container,
        list_key: Option<Field>,
        key: Key,
    ) -> PathId {
        let target = self.container_get(Some(container), key);
        let existing = target.and_then(|t| self.cached_paths(parent).map.get(&t).copied());
        let path = match existing {
            Some(p) => p,
            None => {
                let id = self.paths.len() as PathId;
                self.paths.push(PathData {
                    parent_path: None,
                    parent,
                    container: None,
                    list_key: None,
                    key: Key::Null,
                    node: None,
                    type_node: None,
                    contexts: Vec::new(),
                    context: None,
                    flags: 0,
                    created_under: parent,
                });
                if let Some(t) = target {
                    let entry = self.cached_paths(parent);
                    entry.map.insert(t, id);
                    entry.all.push(id);
                }
                id
            }
        };
        self.setup(path, parent_path, Some(container), list_key, key);
        path
    }

    /// `setup(parentPath, container, listKey, key)`.
    fn setup(
        &mut self,
        p: PathId,
        parent_path: Option<PathId>,
        container: Option<Container>,
        list_key: Option<Field>,
        key: Key,
    ) {
        let d = &mut self.paths[p as usize];
        d.list_key = list_key;
        d.container = container;
        if parent_path.is_some() {
            d.parent_path = parent_path;
        }
        self.set_key(p, key);
    }

    /// `setKey(key)`: key, node = container[key], type.
    fn set_key(&mut self, p: PathId, key: Key) {
        let node = self.container_get(self.paths[p as usize].container, key);
        let d = &mut self.paths[p as usize];
        d.key = key;
        d.node = node;
        d.type_node = node;
    }

    // -- contexts (path/context.js) ------------------------------------------------

    fn set_context(&mut self, p: PathId, ctx: Option<CtxId>) {
        let d = &mut self.paths[p as usize];
        d.flags = 0;
        if ctx.is_some() {
            d.context = ctx;
        }
        self.set_scope(p);
    }

    // -- scopes (scope/index.js: the Scope cache, init, crawl) ----------------

    /// `setScope()`: a scope node's path gets its Scope — the cached one
    /// when it was made for this very path, else a NEW one, which `init()`
    /// crawls (unless an enclosing scope is crawling). The crawl is a full
    /// traversal of the node's subtree, and it matters here for one side
    /// effect: `NodePath.get` re-parents every cached path it reaches, so a
    /// path requeued before the crawl is visited after it with its parent
    /// as the crawl reached it (a statement wrapped in a new block is seen
    /// through its new path under the block).
    fn set_scope(&mut self, p: PathId) {
        let Some(node) = self.node(p) else {
            return;
        };
        let parent = self.paths[p as usize].parent;
        if !is_scope(self.kind(node), self.kind(parent)) {
            return;
        }
        match self.scope_owner.get(&node) {
            Some(ScopeOwner::Path(q)) if *q == p => return,
            Some(ScopeOwner::Original(pn)) if self.paths[p as usize].created_under == *pn => {
                self.scope_owner.insert(node, ScopeOwner::Path(p));
                return;
            }
            _ => {}
        }
        self.scope_owner.insert(node, ScopeOwner::Path(p));
        if !self.no_crawl && !self.ancestor_crawling(p) {
            self.crawl(p);
        }
    }

    /// `crawl()`'s early exit: an enclosing scope (up to the program) is
    /// crawling. Scopes are found through the path chain (`scope.parent`).
    fn ancestor_crawling(&self, p: PathId) -> bool {
        let mut path = p;
        loop {
            if self.path_is(path, |k| matches!(k, Kind::Program { .. })) {
                return false;
            }
            let skip_method = matches!(self.key(path), Key::Field(Field::Key));
            let Some(mut up) = self.parent_path(path) else {
                return false;
            };
            if skip_method && self.path_is(up, is_method) {
                match self.parent_path(up) {
                    Some(pp) => up = pp,
                    None => return false,
                }
            }
            path = up;
            let is_scope_path = self.node(path).is_some_and(|n| {
                is_scope(self.kind(n), self.kind(self.paths[path as usize].parent))
            });
            if is_scope_path && self.crawling.contains(&path) {
                return true;
            }
        }
    }

    /// `crawl()`: traverse the scope node's subtree with the collector
    /// visitor (which only reads).
    fn crawl(&mut self, p: PathId) {
        let Some(node) = self.node(p) else {
            return;
        };
        self.crawling.insert(p);
        // The collector never mutates, so its traversal cannot fail.
        let _ = self.traverse_node(node, Some(p), true);
        self.crawling.remove(&p);
    }

    fn push_context(&mut self, p: PathId, ctx: CtxId) {
        self.paths[p as usize].contexts.push(ctx);
        self.set_context(p, Some(ctx));
    }

    fn pop_context(&mut self, p: PathId) {
        self.paths[p as usize].contexts.pop();
        let top = self.paths[p as usize].contexts.last().copied();
        self.set_context(p, top);
    }

    /// `resync()`.
    fn resync(&mut self, p: PathId) {
        if self.removed(p) {
            return;
        }
        // _resyncParent
        if let Some(pp) = self.paths[p as usize].parent_path
            && let Some(pn) = self.paths[pp as usize].node
        {
            self.paths[p as usize].parent = pn;
        } else if let Some(pp) = self.paths[p as usize].parent_path {
            // parentPath.node is null: `this.parent = null` — keep the old
            // id but the list resync below then finds no container.
            let _ = pp;
        }
        // _resyncList
        let d = &self.paths[p as usize];
        if let Some(lk) = d.list_key {
            let parent = d.parent;
            let has = matches!(self.kind(parent).get(lk), Slot::Many(_));
            self.paths[p as usize].container = if has {
                Some(Container::List(parent, lk))
            } else {
                None
            };
        }
        // _resyncKey
        let d = &self.paths[p as usize];
        let (container, key, node) = (d.container, d.key, d.node);
        let Some(c) = container else {
            return;
        };
        // `this.node === this.container[this.key]` (null/undefined alike).
        if self.container_get(Some(c), key) == node {
            return;
        }
        match c {
            Container::List(..) => {
                let found = self
                    .list(c)
                    .iter()
                    .position(|&n| node.is_some_and(|x| x == n));
                if let Some(i) = found {
                    self.set_key(p, Key::Index(i));
                    return;
                }
            }
            Container::Node(n) => {
                let keys = self.kind(n).visitor_keys();
                for &f in keys {
                    if node.is_some() && self.container_get(Some(c), Key::Field(f)) == node {
                        self.set_key(p, Key::Field(f));
                        return;
                    }
                }
            }
        }
        self.paths[p as usize].key = Key::Null;
    }

    fn new_queue(&mut self, items: Vec<PathId>) -> QueueId {
        self.queues.push(items);
        (self.queues.len() - 1) as QueueId
    }

    fn new_ctx(&mut self, parent_path: Option<PathId>, crawl: bool) -> CtxId {
        let priority = self.new_queue(Vec::new());
        self.ctxs.push(CtxData {
            queue: None,
            priority,
            parent_path,
            crawl,
        });
        (self.ctxs.len() - 1) as CtxId
    }

    /// `context.maybeQueue(path, notPriority)`.
    fn maybe_queue(&mut self, ctx: CtxId, path: PathId, not_priority: bool) {
        let c = &self.ctxs[ctx as usize];
        if let Some(q) = c.queue {
            let target = if not_priority { q } else { c.priority };
            self.queues[target as usize].push(path);
        }
    }

    /// `requeue(pathToQueue = this)`.
    fn requeue(&mut self, p: PathId, to_queue: PathId) {
        if self.removed(to_queue) || self.no_requeue {
            return;
        }
        let contexts = self.paths[p as usize].contexts.clone();
        for ctx in contexts {
            self.maybe_queue(ctx, to_queue, self.requeue_deferred);
        }
    }

    /// `_getQueueContexts()`.
    fn queue_contexts(&self, p: PathId) -> Vec<CtxId> {
        let mut path = p;
        let mut contexts = &self.paths[p as usize].contexts;
        while contexts.is_empty() {
            match self.paths[path as usize].parent_path {
                Some(pp) => {
                    path = pp;
                    contexts = &self.paths[pp as usize].contexts;
                }
                None => break,
            }
        }
        contexts.clone()
    }

    // -- traversal (context.js, traverse-node.js) ----------------------------------

    /// `shouldVisit(node)` for the context's visitor.
    fn should_visit(&self, ctx: CtxId, node: NodeId) -> bool {
        let kind = self.kind(node);
        let has = if self.ctxs[ctx as usize].crawl {
            crawl_visits(kind)
        } else {
            self.visitor.has(kind)
        };
        if has {
            return true;
        }
        kind.visitor_keys()
            .iter()
            .any(|&k| !matches!(kind.get(k), Slot::One(None)))
    }

    /// `traverseNode(node, opts, scope, state, path)`.
    fn traverse_node(&mut self, node: NodeId, parent_path: Option<PathId>, crawl: bool) -> R<bool> {
        let keys = self.kind(node).visitor_keys();
        if keys.is_empty() {
            return Ok(false);
        }
        let ctx = self.new_ctx(parent_path, crawl);
        for &key in keys {
            if self.ctx_visit(ctx, node, key)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// `context.visit(node, key)`.
    fn ctx_visit(&mut self, ctx: CtxId, node: NodeId, key: Field) -> R<bool> {
        match self.kind(node).get(key) {
            Slot::One(None) => Ok(false),
            Slot::One(Some(child)) => {
                if !self.should_visit(ctx, child) {
                    return Ok(false);
                }
                let pp = self.ctxs[ctx as usize].parent_path;
                let path = self.path_get(pp, node, Container::Node(node), None, Key::Field(key));
                let q = self.new_queue(vec![path]);
                self.visit_queue(ctx, q)
            }
            Slot::Many(items) => {
                if items.is_empty() {
                    return Ok(false);
                }
                let items = items.to_vec();
                let pp = self.ctxs[ctx as usize].parent_path;
                let mut queue = Vec::new();
                for (i, &child) in items.iter().enumerate() {
                    if !child.is_none() && self.should_visit(ctx, child) {
                        queue.push(self.path_get(
                            pp,
                            node,
                            Container::List(node, key),
                            Some(key),
                            Key::Index(i),
                        ));
                    }
                }
                let q = self.new_queue(queue);
                self.visit_queue(ctx, q)
            }
        }
    }

    /// `visitQueue(queue)`.
    fn visit_queue(&mut self, ctx: CtxId, queue: QueueId) -> R<bool> {
        self.ctxs[ctx as usize].queue = Some(queue);
        let prio = self.new_queue(Vec::new());
        self.ctxs[ctx as usize].priority = prio;
        let mut visited: HashSet<NodeId, Fx> = HashSet::default();
        let mut stop = false;
        let mut i = 0usize;
        while i < self.queues[queue as usize].len() {
            let path = self.queues[queue as usize][i];
            i += 1;
            self.resync(path);
            let top = self.paths[path as usize].contexts.last().copied();
            if top != Some(ctx) {
                self.push_context(path, ctx);
            }
            if self.paths[path as usize].key == Key::Null {
                continue;
            }
            let node = self.paths[path as usize].node;
            if let Some(n) = node {
                if visited.contains(&n) {
                    continue;
                }
                visited.insert(n);
            }
            if self.visit(path)? {
                stop = true;
                break;
            }
            let pq = self.ctxs[ctx as usize].priority;
            if !self.queues[pq as usize].is_empty() {
                stop = self.visit_queue(ctx, pq)?;
                let fresh = self.new_queue(Vec::new());
                self.ctxs[ctx as usize].priority = fresh;
                self.ctxs[ctx as usize].queue = Some(queue);
                if stop {
                    break;
                }
            }
        }
        for j in 0..i {
            let p = self.queues[queue as usize][j];
            self.pop_context(p);
        }
        self.ctxs[ctx as usize].queue = None;
        Ok(stop)
    }

    /// `path.visit()`.
    fn visit(&mut self, p: PathId) -> R<bool> {
        if self.node(p).is_none() {
            return Ok(false);
        }
        let current = self.paths[p as usize].context;
        if self.should_skip(p) || self.call(p, false)? {
            return Ok(self.should_stop(p));
        }
        self.paths[p as usize].context = current;
        let node = self.node(p).expect("checked above");
        let crawl = current.is_some_and(|c| self.ctxs[c as usize].crawl);
        let stop = self.traverse_node(node, Some(p), crawl)?;
        self.set_should_stop(p, stop);
        self.paths[p as usize].context = current;
        self.call(p, true)?;
        Ok(self.should_stop(p))
    }

    /// `call("enter" | "exit")`.
    fn call(&mut self, p: PathId, exit: bool) -> R<bool> {
        let Some(node) = self.node(p) else {
            return Ok(false);
        };
        // A crawl's collector only reads: no stage-6 fn runs in it.
        if self.paths[p as usize]
            .context
            .is_some_and(|c| self.ctxs[c as usize].crawl)
        {
            return Ok(false);
        }
        let kind = self.kind(node);
        let fns: Vec<VisitFn> = if exit {
            self.visitor.exit(kind).to_vec()
        } else {
            self.visitor.enter(kind).to_vec()
        };
        self.call_fns(p, &fns)
    }

    /// `_call(fns)`.
    fn call_fns(&mut self, p: PathId, fns: &[VisitFn]) -> R<bool> {
        for f in fns {
            let Some(node) = self.node(p) else {
                return Ok(true);
            };
            f(self, p)?;
            if self.node(p) != Some(node) {
                return Ok(true);
            }
            if self.flags(p) > 0 {
                return Ok(true);
            }
        }
        Ok(false)
    }

    // -- family (path/family.js) ---------------------------------------------------

    /// `path.get(key)` for a single-node field (`setContext(this.context)`).
    pub fn get(&mut self, p: PathId, field: Field) -> R<PathId> {
        let node = self.node(p).ok_or("get on a removed path")?;
        let child = self.path_get(
            Some(p),
            node,
            Container::Node(node),
            None,
            Key::Field(field),
        );
        let ctx = self.paths[p as usize].context;
        self.set_context(child, ctx);
        Ok(child)
    }

    /// `getSibling(key)`.
    fn get_sibling(&mut self, p: PathId, key: Key) -> PathId {
        let d = &self.paths[p as usize];
        let (pp, parent, container, list_key, ctx) =
            (d.parent_path, d.parent, d.container, d.list_key, d.context);
        let container = container.unwrap_or(Container::Node(parent));
        let sib = self.path_get(pp, parent, container, list_key, key);
        self.set_context(sib, ctx);
        sib
    }

    /// `path.set(key, node)` (a plain field write).
    pub fn set(&mut self, p: PathId, field: Field, value: NodeId) -> R<()> {
        let node = self.node(p).ok_or("set on a removed path")?;
        self.tree.kind_mut(node).set(field, Some(value))
    }

    // -- scope ---------------------------------------------------------------------

    /// `path.scope.hasBinding(name, { noGlobals: true })` for `undefined`:
    /// any scope on the path's chain (itself included) declares it.
    pub fn has_undefined_binding(&self, p: PathId) -> bool {
        if self.undefined_scopes.is_empty() {
            return false;
        }
        let mut cur = Some(p);
        while let Some(q) = cur {
            if let Some(n) = self.node(q)
                && self.undefined_scopes.contains(&n)
            {
                return true;
            }
            cur = self.parent_path(q);
        }
        false
    }

    // -- comments --------------------------------------------------------------------

    fn comments_mut(&mut self, n: NodeId) -> &mut NodeComments {
        self.tree
            .node_mut(n)
            .comments
            .get_or_insert_with(Default::default)
    }

    /// `t.inheritsComments(child, parent)` then `t.removeComments(parent)`:
    /// the child ends with the three arrays (possibly empty — Babel's
    /// `_inherit` always assigns), the parent with none.
    fn inherit_and_remove_comments(&mut self, child: NodeId, parent: NodeId) {
        let from = self
            .tree
            .node_mut(parent)
            .comments
            .take()
            .unwrap_or_default();
        let to = self.comments_mut(child);
        union_into(&mut to.trailing, &from.trailing);
        union_into(&mut to.leading, &from.leading);
        union_into(&mut to.inner, &from.inner);
    }

    /// `path.shareCommentsWithSiblings()`.
    fn share_comments_with_siblings(&mut self, p: PathId) {
        let Key::Index(k) = self.key(p) else {
            return;
        };
        let Some(node) = self.node(p) else {
            return;
        };
        // Present arrays (even empty ones, after an inherit) are truthy.
        let (leading, trailing) = match self.tree.comments_of(node) {
            Some(c) => (c.leading.clone(), c.trailing.clone()),
            None => return,
        };
        let prev = if k == 0 {
            None
        } else {
            let s = self.get_sibling(p, Key::Index(k - 1));
            self.node(s)
        };
        let next = {
            let s = self.get_sibling(p, Key::Index(k + 1));
            self.node(s)
        };
        if let Some(prev) = prev {
            if !leading.is_empty() {
                let existing = self
                    .tree
                    .comments_of(prev)
                    .map(|c| c.trailing.clone())
                    .unwrap_or_default();
                let add: Vec<u32> = leading
                    .iter()
                    .copied()
                    .filter(|c| !existing.contains(c))
                    .collect();
                self.comments_mut(prev).trailing.extend(add);
            }
            if !trailing.is_empty() && next.is_none() {
                self.comments_mut(prev)
                    .trailing
                    .extend(trailing.iter().copied());
            }
        }
        if let Some(next) = next {
            if !trailing.is_empty() {
                let existing = self
                    .tree
                    .comments_of(next)
                    .map(|c| c.leading.clone())
                    .unwrap_or_default();
                let add: Vec<u32> = trailing
                    .iter()
                    .copied()
                    .filter(|c| !existing.contains(c))
                    .collect();
                let lead = &mut self.comments_mut(next).leading;
                let mut merged = add;
                merged.extend(lead.iter().copied());
                *lead = merged;
            }
            if !leading.is_empty() && prev.is_none() {
                let lead = &mut self.comments_mut(next).leading;
                let mut merged = leading.clone();
                merged.extend(lead.iter().copied());
                *lead = merged;
            }
        }
    }

    // -- replacement (path/replacement.js) -------------------------------------------

    /// `path.replaceWith(replacement)`.
    pub fn replace_with(&mut self, p: PathId, replacement: NodeId) -> R<()> {
        self.resync(p);
        if self.removed(p) {
            return Err("You can't replace this node, we've already removed it".into());
        }
        if self.node(p) == Some(replacement) {
            return Ok(());
        }
        let mut replacement = replacement;
        if self.type_is(p, Kind::is_statement) && self.kind(replacement).is_expression() {
            let key = self.key(p);
            let pp_is_for = self
                .parent_path(p)
                .is_some_and(|pp| self.path_is(pp, is_for));
            let can_have_var_or_expr =
                matches!(key, Key::Field(Field::Init | Field::Left)) && pp_is_for;
            let pp_arrow = self.parent_path(p).is_some_and(|pp| {
                self.path_is(pp, |k| matches!(k, Kind::ArrowFunctionExpression(_)))
            });
            let can_swap = key == Key::Field(Field::Body)
                && pp_arrow
                && self.path_is(p, |k| matches!(k, Kind::BlockStatement { .. }));
            let pp_export_default = self.parent_path(p).is_some_and(|pp| {
                self.path_is(pp, |k| matches!(k, Kind::ExportDefaultDeclaration { .. }))
            });
            if !can_have_var_or_expr && !can_swap && !pp_export_default {
                replacement = self.tree.synth(Kind::ExpressionStatement {
                    expression: replacement,
                });
            }
        }
        if self.type_is(p, Kind::is_expression) && self.kind(replacement).is_statement() {
            return Err(
                "replaceExpressionWithStatements is not ported (no stage-6 visitor reaches it)"
                    .into(),
            );
        }
        if let Some(old) = self.node(p) {
            self.inherit_and_remove_comments(replacement, old);
        }
        self.replace_with_raw(p, Some(replacement))?;
        self.paths[p as usize].type_node = Some(replacement);
        self.set_scope(p);
        self.requeue(p, p);
        Ok(())
    }

    /// `_replaceWith(node)`: `validate(this.parent, this.key, node)`, the
    /// cache move, the container write. The validation reads the path's
    /// RESYNCED parent, which after a wrap is the new block while the
    /// container is still the wrapped statement — a write Babel rejects
    /// (and so the TS stage throws).
    fn replace_with_raw(&mut self, p: PathId, node: Option<NodeId>) -> R<()> {
        let d = self.paths[p as usize].clone();
        let container = d.container.ok_or("Container is falsy")?;
        let parent = d.parent;
        if let Key::Field(f) = d.key {
            let shape = self.tree.kind_mut(parent).field_shape(f);
            let ty = self.kind(parent).type_name();
            match (shape, node) {
                (FieldShape::List, _) => {
                    return Err(format!(
                        "Property {f:?} of {ty} expected type of array but got {}",
                        if node.is_some() { "object" } else { "null" }
                    ));
                }
                (FieldShape::Required, None) => {
                    return Err(format!(
                        "Property {f:?} of {ty} expected a node but instead got null"
                    ));
                }
                _ => {}
            }
        }
        {
            // `.set(node, this).delete(this.node)` (a null key is never
            // looked up by the stage-6 visitors; it is not stored).
            let entry = self.cached_paths(parent);
            if let Some(n) = node {
                entry.map.insert(n, p);
            }
            if let Some(old) = d.node {
                entry.map.remove(&old);
            }
        }
        self.paths[p as usize].node = node;
        match (container, d.key) {
            (Container::Node(n), Key::Field(f)) => self.tree.kind_mut(n).set(f, node)?,
            (Container::List(..), Key::Index(i)) => {
                let list = self.list_mut(container)?;
                if i < list.len() {
                    list[i] = node.unwrap_or(NodeId::NONE);
                } else {
                    return Err("replace past the end of a list".into());
                }
            }
            _ => return Err("replace on a keyless path".into()),
        }
        Ok(())
    }

    /// `path.replaceWithMultiple(nodes)`.
    pub fn replace_with_multiple(&mut self, p: PathId, nodes: Vec<NodeId>) -> R<()> {
        self.resync(p);
        let first = *nodes.first().ok_or("replaceWithMultiple with no nodes")?;
        let last = *nodes.last().expect("non-empty");
        if let Some(old) = self.node(p) {
            // inheritLeadingComments / inheritTrailingComments: the arrays
            // are assigned even when empty.
            let c = self.tree.comments_of(old).cloned().unwrap_or_default();
            union_into(&mut self.comments_mut(first).leading, &c.leading);
            union_into(&mut self.comments_mut(last).trailing, &c.trailing);
            let parent = self.paths[p as usize].parent;
            self.cached_paths(parent).map.remove(&old);
        }
        // `this.node = this.container[this.key] = null`
        self.paths[p as usize].node = None;
        let d = self.paths[p as usize].clone();
        match (d.container, d.key) {
            (Some(Container::List(..)), Key::Index(i)) => {
                // A JS array grows when written past its end (pushContainer).
                let list = self.list_mut(d.container.expect("some"))?;
                if i >= list.len() {
                    list.resize(i + 1, NodeId::NONE);
                }
                list[i] = NodeId::NONE;
            }
            (Some(Container::Node(n)), Key::Field(f)) => self.tree.kind_mut(n).set(f, None)?,
            _ => return Err("replaceWithMultiple on a keyless path".into()),
        }
        self.insert_after(p, nodes)?;
        if self.node(p).is_some() {
            self.requeue(p, p);
        } else {
            self.remove(p)?;
        }
        Ok(())
    }

    // -- modification (path/modification.js) -----------------------------------------

    /// `path.insertBefore(nodes)`.
    pub fn insert_before(&mut self, p: PathId, nodes: Vec<NodeId>) -> R<()> {
        if self.removed(p) {
            return Err("NodePath has been removed so is read-only.".into());
        }
        let pp = self.parent_path(p).ok_or("insertBefore at the root")?;
        let parent = self.paths[p as usize].parent;
        if self.path_is(pp, |k| {
            matches!(
                k,
                Kind::ExpressionStatement { .. } | Kind::LabeledStatement { .. }
            )
        }) || matches!(self.kind(parent), Kind::ExportNamedDeclaration { .. })
            || (self.path_is(pp, |k| matches!(k, Kind::ExportDefaultDeclaration { .. }))
                && self.path_is(p, is_declaration))
        {
            return self.insert_before(pp, nodes);
        }
        let for_init = self.path_is(pp, |k| matches!(k, Kind::ForStatement { .. }))
            && self.key(p) == Key::Field(Field::Init);
        if self.type_is(p, Kind::is_expression) || for_init {
            return Err("insertBefore on an expression is not ported".into());
        }
        if matches!(self.paths[p as usize].container, Some(Container::List(..))) {
            let Key::Index(k) = self.key(p) else {
                return Err("list path without an index".into());
            };
            self.container_insert(p, k, nodes)?;
            return Ok(());
        }
        if self.is_statement_or_block(p) {
            let block = self.wrap_in_block(p)?;
            return self.unshift_container(block, Field::Body, nodes);
        }
        Err("We don't know what to do with this node type. We were previously a Statement but we can't fit in here?".into())
    }

    /// `path.insertAfter(nodes)`.
    fn insert_after(&mut self, p: PathId, nodes: Vec<NodeId>) -> R<()> {
        if self.removed(p) {
            return Err("NodePath has been removed so is read-only.".into());
        }
        if self.path_is(p, |k| matches!(k, Kind::SequenceExpression { .. })) {
            return Err("insertAfter on a sequence is not ported".into());
        }
        let pp = self.parent_path(p).ok_or("insertAfter at the root")?;
        let parent = self.paths[p as usize].parent;
        if self.path_is(pp, |k| {
            matches!(
                k,
                Kind::ExpressionStatement { .. } | Kind::LabeledStatement { .. }
            )
        }) || matches!(self.kind(parent), Kind::ExportNamedDeclaration { .. })
            || (self.path_is(pp, |k| matches!(k, Kind::ExportDefaultDeclaration { .. }))
                && self.path_is(p, is_declaration))
        {
            let wrapped = nodes
                .into_iter()
                .map(|n| {
                    if self.kind(n).is_expression() {
                        self.tree.synth(Kind::ExpressionStatement { expression: n })
                    } else {
                        n
                    }
                })
                .collect();
            return self.insert_after(pp, wrapped);
        }
        let for_init = self.path_is(pp, |k| matches!(k, Kind::ForStatement { .. }))
            && self.key(p) == Key::Field(Field::Init);
        if self.type_is(p, Kind::is_expression) || for_init {
            return Err("insertAfter on an expression is not ported".into());
        }
        if matches!(self.paths[p as usize].container, Some(Container::List(..))) {
            let Key::Index(k) = self.key(p) else {
                return Err("list path without an index".into());
            };
            self.container_insert(p, k + 1, nodes)?;
            return Ok(());
        }
        if self.is_statement_or_block(p) {
            let block = self.wrap_in_block(p)?;
            return self.push_container(block, Field::Body, nodes);
        }
        Err("We don't know what to do with this node type. We were previously a Statement but we can't fit in here?".into())
    }

    /// The `isStatementOrBlock` branch's first half: replace the path's
    /// node with a block holding it (or nothing, when the node is null or
    /// an empty expression statement); the block's path.
    fn wrap_in_block(&mut self, p: PathId) -> R<PathId> {
        // An expression statement always has its expression here.
        let body = self.node(p).into_iter().collect();
        let block = self.tree.synth(Kind::BlockStatement {
            directives: Vec::new(),
            body,
        });
        self.replace_with(p, block)?;
        Ok(p)
    }

    /// `isStatementOrBlock()`.
    fn is_statement_or_block(&self, p: PathId) -> bool {
        let Some(pp) = self.parent_path(p) else {
            return false;
        };
        if self.path_is(pp, |k| matches!(k, Kind::LabeledStatement { .. })) {
            return false;
        }
        if let Some(Container::Node(c)) = self.paths[p as usize].container
            && matches!(self.kind(c), Kind::BlockStatement { .. })
        {
            return false;
        }
        matches!(self.key(p), Key::Field(f) if f.is_statement_or_block_key())
    }

    /// `_containerInsert(from, nodes)`.
    fn container_insert(&mut self, p: PathId, from: usize, nodes: Vec<NodeId>) -> R<Vec<PathId>> {
        self.update_sibling_keys(p, from, nodes.len() as isize);
        let container = self.paths[p as usize].container.ok_or("no container")?;
        {
            let list = self.list_mut(container)?;
            if from > list.len() {
                return Err("insert past the end of a list".into());
            }
            list.splice(from..from, nodes.iter().copied());
        }
        let ctx = self.paths[p as usize].context;
        let mut paths = Vec::with_capacity(nodes.len());
        for i in 0..nodes.len() {
            let sib = self.get_sibling(p, Key::Index(from + i));
            paths.push(sib);
            if let Some(c) = ctx
                && self.ctxs[c as usize].queue.is_some()
            {
                self.push_context(sib, c);
            }
        }
        let contexts = self.queue_contexts(p);
        for &path in &paths {
            for &c in &contexts {
                self.maybe_queue(c, path, true);
            }
        }
        Ok(paths)
    }

    /// `updateSiblingKeys(fromIndex, incrementBy)`.
    fn update_sibling_keys(&mut self, p: PathId, from: usize, by: isize) {
        let parent = self.paths[p as usize].parent;
        let container = self.paths[p as usize].container;
        let Some(entry) = self.cache.get(&parent) else {
            return;
        };
        let live: Vec<PathId> = entry
            .all
            .iter()
            .copied()
            .filter(|&q| {
                self.paths[q as usize]
                    .node
                    .is_some_and(|n| entry.map.get(&n) == Some(&q))
            })
            .collect();
        for q in live {
            let d = &mut self.paths[q as usize];
            if let Key::Index(k) = d.key
                && d.container == container
                && k >= from
            {
                d.key = Key::Index((k as isize + by) as usize);
            }
        }
    }

    /// `unshiftContainer(listKey, nodes)`.
    fn unshift_container(&mut self, p: PathId, field: Field, nodes: Vec<NodeId>) -> R<()> {
        let node = self.node(p).ok_or("unshiftContainer on a removed path")?;
        let first = self.path_get(
            Some(p),
            node,
            Container::List(node, field),
            Some(field),
            Key::Index(0),
        );
        let ctx = self.paths[p as usize].context;
        self.set_context(first, ctx);
        self.container_insert(first, 0, nodes)?;
        Ok(())
    }

    /// `pushContainer(listKey, nodes)`.
    fn push_container(&mut self, p: PathId, field: Field, nodes: Vec<NodeId>) -> R<()> {
        let node = self.node(p).ok_or("pushContainer on a removed path")?;
        let len = self.list(Container::List(node, field)).len();
        let end = self.path_get(
            Some(p),
            node,
            Container::List(node, field),
            Some(field),
            Key::Index(len),
        );
        let ctx = self.paths[p as usize].context;
        self.set_context(end, ctx);
        self.replace_with_multiple(end, nodes)
    }

    // -- removal (path/removal.js + lib/removal-hooks.js) ------------------------------

    /// `path.remove()`.
    pub fn remove(&mut self, p: PathId) -> R<()> {
        if self.removed(p) {
            return Err("NodePath has been removed so is read-only.".into());
        }
        self.resync(p);
        if self.call_removal_hooks(p)? {
            self.mark_removed(p);
            return Ok(());
        }
        self.share_comments_with_siblings(p);
        self.remove_raw(p)?;
        self.mark_removed(p);
        Ok(())
    }

    fn call_removal_hooks(&mut self, p: PathId) -> R<bool> {
        let Some(pp) = self.parent_path(p) else {
            return Ok(false);
        };
        let key = self.key(p);
        let list_key = self.list_key(p);
        let pk = self.node(pp).map(|n| self.kind(n).clone());
        let Some(pk) = pk else {
            return Ok(false);
        };
        // 1: remove the parent instead
        let remove_parent = (key == Key::Field(Field::Test)
            && matches!(
                pk,
                Kind::WhileStatement { .. }
                    | Kind::DoWhileStatement { .. }
                    | Kind::SwitchCase { .. }
            ))
            || (key == Key::Field(Field::Declaration)
                && matches!(
                    pk,
                    Kind::ExportNamedDeclaration { .. }
                        | Kind::ExportDefaultDeclaration { .. }
                        | Kind::ExportAllDeclaration { .. }
                ))
            || (key == Key::Field(Field::Body) && matches!(pk, Kind::LabeledStatement { .. }))
            || (list_key == Some(Field::Declarations)
                && matches!(&pk, Kind::VariableDeclaration { declarations, .. } if declarations.len() == 1))
            || (key == Key::Field(Field::Expression)
                && matches!(pk, Kind::ExpressionStatement { .. }));
        if remove_parent {
            self.remove(pp)?;
            return Ok(true);
        }
        // 2: a one-expression sequence becomes its expression
        if let Kind::SequenceExpression { expressions } = &pk
            && expressions.len() == 1
        {
            self.replace_with(pp, expressions[0])?;
            return Ok(true);
        }
        // 3: a binary loses one side
        if let Kind::BinaryExpression(b) | Kind::LogicalExpression(b) = &pk {
            let keep = if key == Key::Field(Field::Left) {
                b.right
            } else {
                b.left
            };
            self.replace_with(pp, keep)?;
            return Ok(true);
        }
        // 4: an if consequent / loop or arrow body becomes an empty block
        let loop_or_arrow = matches!(
            pk,
            Kind::DoWhileStatement { .. }
                | Kind::ForInStatement { .. }
                | Kind::ForStatement { .. }
                | Kind::WhileStatement { .. }
                | Kind::ForOfStatement { .. }
                | Kind::ArrowFunctionExpression(_)
        );
        if (matches!(pk, Kind::IfStatement { .. }) && key == Key::Field(Field::Consequent))
            || (key == Key::Field(Field::Body) && loop_or_arrow)
        {
            let block = self.tree.synth(Kind::BlockStatement {
                directives: Vec::new(),
                body: Vec::new(),
            });
            self.replace_with(p, block)?;
            return Ok(true);
        }
        Ok(false)
    }

    /// `_remove()`.
    fn remove_raw(&mut self, p: PathId) -> R<()> {
        let d = self.paths[p as usize].clone();
        match (d.container, d.key) {
            (Some(c @ Container::List(..)), Key::Index(i)) => {
                self.list_mut(c)?.remove(i);
                self.update_sibling_keys(p, i, -1);
                Ok(())
            }
            _ => self.replace_with_raw(p, None),
        }
    }

    /// `_markRemoved()`.
    fn mark_removed(&mut self, p: PathId) {
        self.paths[p as usize].flags |= SHOULD_SKIP | REMOVED;
        let parent = self.paths[p as usize].parent;
        let node = self.paths[p as usize].node;
        if let Some(entry) = self.cache.get_mut(&parent)
            && let Some(n) = node
        {
            entry.map.remove(&n);
        }
        self.paths[p as usize].node = None;
    }

    /// Allocate a synthesized node (a `t.*` builder).
    pub fn build(&mut self, kind: Kind) -> NodeId {
        self.tree.alloc(Node::synth(kind))
    }
}

/// `t.isScope(node, parent)`: a Scopable node, except a block that is a
/// function's or catch clause's body; a pattern that is a function's or
/// catch clause's parameter is one.
fn is_scope(node: &Kind, parent: &Kind) -> bool {
    let fn_or_catch = parent.is_function() || matches!(parent, Kind::CatchClause { .. });
    match node {
        Kind::BlockStatement { .. } if fn_or_catch => false,
        Kind::ObjectPattern { .. } | Kind::ArrayPattern { .. } | Kind::AssignmentPattern { .. } => {
            fn_or_catch
        }
        Kind::BlockStatement { .. }
        | Kind::CatchClause { .. }
        | Kind::DoWhileStatement { .. }
        | Kind::ForInStatement { .. }
        | Kind::ForStatement { .. }
        | Kind::FunctionDeclaration(_)
        | Kind::FunctionExpression(_)
        | Kind::Program { .. }
        | Kind::ObjectMethod(_)
        | Kind::SwitchStatement { .. }
        | Kind::WhileStatement { .. }
        | Kind::ArrowFunctionExpression(_)
        | Kind::ClassExpression(_)
        | Kind::ClassDeclaration(_)
        | Kind::ForOfStatement { .. }
        | Kind::ClassMethod(_)
        | Kind::ClassPrivateMethod(_)
        | Kind::StaticBlock { .. } => true,
        _ => false,
    }
}

fn is_method(k: &Kind) -> bool {
    matches!(
        k,
        Kind::ObjectMethod(_) | Kind::ClassMethod(_) | Kind::ClassPrivateMethod(_)
    )
}

/// The node types the crawl's merged visitor (`Scope` + `collectorVisitor`,
/// exploded) names — `shouldVisit` is true for these even without
/// children.
fn crawl_visits(k: &Kind) -> bool {
    matches!(
        k,
        Kind::ArrayPattern { .. }
            | Kind::ArrowFunctionExpression(_)
            | Kind::AssignmentExpression(_)
            | Kind::AssignmentPattern { .. }
            | Kind::BlockStatement { .. }
            | Kind::CatchClause { .. }
            | Kind::ClassDeclaration(_)
            | Kind::ClassExpression(_)
            | Kind::ClassMethod(_)
            | Kind::ClassPrivateMethod(_)
            | Kind::DoWhileStatement { .. }
            | Kind::ExportAllDeclaration { .. }
            | Kind::ExportDefaultDeclaration { .. }
            | Kind::ExportNamedDeclaration { .. }
            | Kind::ForInStatement { .. }
            | Kind::ForOfStatement { .. }
            | Kind::ForStatement { .. }
            | Kind::FunctionDeclaration(_)
            | Kind::FunctionExpression(_)
            | Kind::Identifier { .. }
            | Kind::ImportDeclaration { .. }
            | Kind::LabeledStatement { .. }
            | Kind::ObjectMethod(_)
            | Kind::ObjectPattern { .. }
            | Kind::Program { .. }
            | Kind::StaticBlock { .. }
            | Kind::SwitchStatement { .. }
            | Kind::UnaryExpression { .. }
            | Kind::UpdateExpression { .. }
            | Kind::VariableDeclaration { .. }
            | Kind::WhileStatement { .. }
    )
}

/// `Array.from(new Set([...to, ...from]))` into `to`.
fn union_into(to: &mut Vec<u32>, from: &[u32]) {
    for c in from {
        if !to.contains(c) {
            to.push(*c);
        }
    }
}

fn is_for(k: &Kind) -> bool {
    matches!(
        k,
        Kind::ForStatement { .. } | Kind::ForInStatement { .. } | Kind::ForOfStatement { .. }
    )
}

/// Babel's `isDeclaration`.
fn is_declaration(k: &Kind) -> bool {
    matches!(
        k,
        Kind::FunctionDeclaration(_)
            | Kind::VariableDeclaration { .. }
            | Kind::ClassDeclaration(_)
            | Kind::ImportDeclaration { .. }
            | Kind::ExportNamedDeclaration { .. }
            | Kind::ExportDefaultDeclaration { .. }
            | Kind::ExportAllDeclaration { .. }
    )
}
