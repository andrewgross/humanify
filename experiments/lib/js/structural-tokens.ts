/**
 * The rename-invariant token stream of a Babel path — binding identifiers as
 * order-keyed slots, everything else verbatim (or literal-blurred). Moved
 * from the TS pipeline's src/analysis/structural-hash.ts
 * (`serializePathTokens`) at the cutover (docs/rust-port/19-cutover.md).
 *
 * Two harness consumers, both needing the pre-cutover bytes exactly:
 * - the vendor churn scorer (exp046 vendor-churn.ts) — it keys vendor
 *   functions by this stream, so the vendor KPI columns on record were
 *   computed by exactly this code;
 * - the clone census (scripts/clone-census.ts), which fingerprints the
 *   harness's own TypeScript with it.
 *
 * The serializer is verbatim, options included; both callers pass only
 * `preserveLiterals`, so the other two options stay at their defaults (off).
 */
import type { Binding, NodePath } from "@babel/traverse";
import * as t from "@babel/types";

type BindingCache = Map<t.Identifier, Binding | null>;

/** Identifier occurrence → resolved binding, per AST (resolution is
 * position-based, so memoizing it for the AST's life is safe). */
const bindingCacheByRoot = new WeakMap<t.Node, BindingCache>();

function bindingCacheFor(path: NodePath): BindingCache {
  let scope = path.scope;
  while (scope.parent) scope = scope.parent;
  const root = scope.path.node;
  let cache = bindingCacheByRoot.get(root);
  if (!cache) {
    cache = new Map();
    bindingCacheByRoot.set(root, cache);
  }
  return cache;
}

function isASTNode(value: unknown): value is t.Node {
  return Boolean(value && typeof value === "object" && "type" in value);
}

const SERIALIZE_SKIP_KEYS = new Set([
  "type",
  "loc",
  "start",
  "end",
  "extra",
  "leadingComments",
  "trailingComments",
  "innerComments",
  // Rename artifact, not structure: renaming a shorthand binding expands
  // {u} → {u: userId} (the key keeps its external name), flipping this
  // flag and changing the hash of every containing function after
  // humanify+regenerate. Key and value are serialized independently, so
  // dropping the flag loses nothing.
  "shorthand"
]);

/**
 * Resolves the binding an identifier occurrence refers to, memoized in the
 * owning AST's cache (resolution is purely position-based, so it is safe to
 * memoize for the AST's whole life — ancestors re-hash the same nested
 * identifiers). Declaration ids of function/class declarations need care:
 * the id's own scope is the function scope, where a same-named param/var
 * would shadow the binding the declaration creates (`function e(e) {}` is
 * common minified output), so resolve those from the parent scope.
 */
function resolveIdentifierBinding(
  p: NodePath<t.Identifier>,
  bindingCache: BindingCache
): Binding | null {
  const cached = bindingCache.get(p.node);
  if (cached !== undefined) return cached;

  const parent = p.parentPath;
  let binding: Binding | null = null;
  if (
    (parent?.isFunctionDeclaration() || parent?.isClassDeclaration()) &&
    parent.node.id === p.node
  ) {
    binding =
      parent.scope.parent?.getBinding(p.node.name) ??
      parent.scope.getBinding(p.node.name) ??
      null;
  } else {
    binding = p.scope.getBinding(p.node.name) ?? null;
  }
  bindingCache.set(p.node, binding);
  return binding;
}

type IdentifierRole = "verbatim" | "label" | "slot";

/**
 * Classifies an identifier occurrence by its structural position:
 * - verbatim: non-computed member property names, object/class member
 *   keys, meta properties — minifier-stable content, never renamed
 * - label: label declarations/references — renamed by minifiers, but not
 *   bindings; normalized in their own namespace
 * - slot: binding references/declarations — normalized per binding
 */
function identifierRole(parent: t.Node | null, key: string): IdentifierRole {
  if (!parent) return "slot";
  if (
    (t.isMemberExpression(parent) || t.isOptionalMemberExpression(parent)) &&
    !parent.computed &&
    key === "property"
  ) {
    return "verbatim";
  }
  if (
    (t.isObjectProperty(parent) ||
      t.isObjectMethod(parent) ||
      t.isClassMethod(parent) ||
      t.isClassProperty(parent)) &&
    !parent.computed &&
    key === "key"
  ) {
    return "verbatim";
  }
  if (t.isMetaProperty(parent)) return "verbatim";
  // External module-API names: `export { local as exported }` /
  // `import { imported as local }`. Renaming the local flips the
  // specifier between shorthand and aliased form while the external name
  // stays fixed — the external side is content (like property keys), not
  // a binding slot, or the flip would change the hash.
  if (t.isExportSpecifier(parent) && key === "exported") return "verbatim";
  if (t.isImportSpecifier(parent) && key === "imported") return "verbatim";
  if (
    (t.isLabeledStatement(parent) ||
      t.isBreakStatement(parent) ||
      t.isContinueStatement(parent)) &&
    key === "label"
  ) {
    return "label";
  }
  return "slot";
}

/** Pre-resolve bindings for every slot-position identifier in the subtree. */
function collectIdentifierBindings(
  rootPath: NodePath,
  bindingCache: BindingCache
): void {
  if (rootPath.isIdentifier()) {
    resolveIdentifierBinding(rootPath as NodePath<t.Identifier>, bindingCache);
  }
  rootPath.traverse({
    Identifier(p: NodePath<t.Identifier>) {
      const role = identifierRole(p.parentPath?.node ?? null, String(p.key));
      if (role === "slot") resolveIdentifierBinding(p, bindingCache);
    }
  });
}

interface SerializeState {
  parts: string[];
  /** Resolved-binding view of the owning AST's cache (read-only here). */
  bindingByIdentifier: BindingCache;
  /**
   * Slot placeholders keyed by the binding's DECLARATION identifier node,
   * not the Binding object: a scope re-crawl (Babel cache clear, fresh
   * traverse) creates new Binding objects for the same declaration, and a
   * walk that mixes cached and freshly-resolved occurrences would split one
   * logical binding into two slots. The declaration node is era-stable.
   */
  slotByDeclId: Map<t.Identifier, string>;
  /** slot → the Binding that claimed it (first resolution wins). */
  bindingBySlot: Map<string, Binding>;
  labelSlots: Map<string, string>;
  /** placeholder → original name, binding slots only */
  mapping: Map<string, string>;
  counter: number;
  preserveLiterals: boolean;
  /**
   * When set, a class-private name serializes as an ORDER-KEYED SLOT
   * (`P=$1`) instead of verbatim (`P=#f`), so a consistent private rename
   * leaves the stream unchanged. Off by default because the same serializer
   * produces `structuralHash`, which is used for cross-version MATCHING —
   * changing it there changes emitted output and is a separate question.
   */
  privateNamesAsSlots?: boolean;
  /** private name → its slot, first occurrence wins. */
  privateSlots?: Map<string, string>;
  /**
   * When set, the local of an `export { x } from "m"` specifier serializes
   * verbatim: it names a binding of ANOTHER module, not a reference here.
   * Resolved by name it read verbatim before a rename and as a slot after
   * one that gave a local binding the same name (finding #34). Off by
   * default for the same reason as `privateNamesAsSlots` — the matching
   * surface shares this serializer.
   */
  reExportLocalsVerbatim?: boolean;
  /** True while serializing inside an `export … from` declaration. */
  inReExport?: boolean;
}

interface SerializeOptions {
  preserveLiterals?: boolean;
  privateNamesAsSlots?: boolean;
  reExportLocalsVerbatim?: boolean;
}

/** The local of a specifier under `export … from` (see reExportLocalsVerbatim). */
function isReExportLocal(
  parent: t.Node | null,
  key: string,
  state: SerializeState
): boolean {
  return (
    state.inReExport === true && t.isExportSpecifier(parent) && key === "local"
  );
}

function serializeIdentifier(
  node: t.Identifier,
  parent: t.Node | null,
  key: string,
  state: SerializeState
): void {
  const role = identifierRole(parent, key);
  if (role === "verbatim" || isReExportLocal(parent, key, state)) {
    state.parts.push(`I=${node.name}`);
    return;
  }
  if (role === "label") {
    let slot = state.labelSlots.get(node.name);
    if (!slot) {
      slot = `L${state.labelSlots.size}`;
      state.labelSlots.set(node.name, slot);
    }
    state.parts.push(slot);
    return;
  }
  const binding = state.bindingByIdentifier.get(node);
  if (!binding) {
    // Free identifier (true global) — version-stable content.
    state.parts.push(`I=${node.name}`);
    return;
  }
  let slot = state.slotByDeclId.get(binding.identifier);
  if (!slot) {
    slot = `$${state.counter++}`;
    state.slotByDeclId.set(binding.identifier, slot);
    state.bindingBySlot.set(slot, binding);
    state.mapping.set(slot, node.name);
  }
  state.parts.push(slot);
}

/**
 * Full-string shapes of machine-generated, per-release literals: version
 * strings, build timestamps, embedded content digests. They change every
 * release — often across LENGTHS ("2.1.99"→"2.1.100") — so the plain
 * length marker would flip an otherwise identical function's hash and
 * demote it from exact-match name reuse. Each class canonicalizes to its
 * own token; digest length is format-stable (git SHA 40, sha256 64) and
 * stays in the token as a discriminator. Shapes are anchored full-string
 * matches so ordinary prose can never canonicalize.
 */
const VOLATILE_SEMVER =
  /^v?\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/;
const VOLATILE_ISO8601 =
  /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:?\d{2})?$/;
const VOLATILE_HEX_DIGEST = /^[0-9a-fA-F]{16,64}$/;

function volatileLiteralToken(value: string): string | null {
  if (VOLATILE_SEMVER.test(value)) return "__VOLATILE_SEMVER__";
  if (VOLATILE_ISO8601.test(value)) return "__VOLATILE_ISO8601__";
  if (VOLATILE_HEX_DIGEST.test(value)) {
    return `__VOLATILE_HEX_${value.length}__`;
  }
  return null;
}

/** String literal hash token: exact when preserving, else volatile class or length. */
function stringLiteralToken(value: string, keep: boolean): string {
  if (keep) return `S=${JSON.stringify(value)}`;
  return `S=${volatileLiteralToken(value) ?? `__STR_${value.length}__`}`;
}

/** Template quasi hash token, same normalization as string literals. */
function templateElementToken(raw: string, keep: boolean): string {
  if (keep) return `Q=${JSON.stringify(raw)}`;
  return `Q=${volatileLiteralToken(raw) ?? raw.length}`;
}

/** Serialize literal node types; returns false when not a literal. */
function serializeLiteral(node: t.Node, state: SerializeState): boolean {
  const keep = state.preserveLiterals;
  if (t.isStringLiteral(node)) {
    state.parts.push(stringLiteralToken(node.value, keep));
    return true;
  }
  if (t.isNumericLiteral(node)) {
    const val = node.value;
    const magnitude = val === 0 ? 0 : Math.floor(Math.log10(Math.abs(val) + 1));
    state.parts.push(keep ? `N=${val}` : `N=${magnitude}`);
    return true;
  }
  if (t.isBigIntLiteral(node)) {
    state.parts.push(keep ? `B=${node.value}` : "B=0");
    return true;
  }
  if (t.isRegExpLiteral(node)) {
    state.parts.push(`R=${node.pattern}/${node.flags}`);
    return true;
  }
  if (t.isTemplateElement(node)) {
    state.parts.push(
      templateElementToken(node.value.raw, keep),
      `,tail=${node.tail}`
    );
    return true;
  }
  return false;
}

/**
 * Statement-position fields where JS allows a bare (unbraced) statement.
 * Function/class bodies and try/catch/finally blocks are inherently
 * blocks and never appear here.
 */
const BARE_STATEMENT_POSITIONS = new Set([
  "IfStatement.consequent",
  "IfStatement.alternate",
  "ForStatement.body",
  "ForInStatement.body",
  "ForOfStatement.body",
  "WhileStatement.body",
  "DoWhileStatement.body",
  "LabeledStatement.body",
  "WithStatement.body"
]);

/**
 * A single-statement block at a bare-statement position serializes as its
 * inner statement — `if (a) { f(); }` and `if (a) f();` are the same
 * structure. Babel's generator block-wraps a bare if-consequent to
 * disambiguate a dangling else, so without this a generate→parse
 * roundtrip of minified input flips hashes and the fresh-parse rename
 * invariant misfires. Blocks whose lone statement is scoping-relevant
 * (let/const/class/function) keep their block marker: the braces change
 * where the binding lives.
 */
function unwrappableBlockStatement(
  node: t.Node,
  parent: t.Node | null,
  key: string
): t.Statement | null {
  if (!t.isBlockStatement(node) || node.body.length !== 1) return null;
  if (!parent || !BARE_STATEMENT_POSITIONS.has(`${parent.type}.${key}`)) {
    return null;
  }
  const only = node.body[0];
  if (
    t.isVariableDeclaration(only) ||
    t.isFunctionDeclaration(only) ||
    t.isClassDeclaration(only)
  ) {
    return null;
  }
  return only;
}

function serializeNode(
  node: t.Node,
  parent: t.Node | null,
  key: string,
  state: SerializeState
): void {
  if (t.isIdentifier(node)) {
    serializeIdentifier(node, parent, key, state);
    return;
  }
  const unwrapped = unwrappableBlockStatement(node, parent, key);
  if (unwrapped) {
    serializeNode(unwrapped, parent, key, state);
    return;
  }
  if (t.isPrivateName(node)) {
    // Class-private names are member keys, not scope bindings; a nested
    // Identifier here must not resolve against same-named var bindings.
    state.parts.push(privateNameToken(node.id.name, state));
    return;
  }
  if (serializeLiteral(node, state)) return;

  // Private names are CLASS-scoped, so each class gets its own slot numbering.
  // Without this, `#f` in two different classes shares one slot, and renaming
  // them to different names makes the output look like it has MORE distinct
  // private names than the input — every later slot shifts. That is exactly
  // what a live run reported: `P=$4` in the original became `P=$8`.
  const outerPrivateSlots = state.privateSlots;
  if (state.privateNamesAsSlots && t.isClass(node)) {
    state.privateSlots = new Map();
  }

  const outerInReExport = state.inReExport;
  if (
    state.reExportLocalsVerbatim &&
    t.isExportNamedDeclaration(node) &&
    node.source
  ) {
    state.inReExport = true;
  }

  state.parts.push(`${node.type}{`);
  for (const k of Object.keys(node)) {
    if (SERIALIZE_SKIP_KEYS.has(k)) continue;
    const value = (node as unknown as Record<string, unknown>)[k];
    if (value === undefined) continue;
    state.parts.push(`${k}:`);
    serializeValue(value, node, k, state);
    state.parts.push(";");
  }
  state.parts.push("}");

  // Restore, so a nested class does not leak its numbering to the enclosing one.
  state.privateSlots = outerPrivateSlots;
  state.inReExport = outerInReExport;
}

/**
 * How a private name appears in the stream.
 *
 * Verbatim by default. Under `privateNamesAsSlots` it becomes a slot keyed by
 * the name's FIRST occurrence WITHIN ITS CLASS, which makes a consistent
 * rename (`#f` -> `#A` at the declaration and every use) stream-identical,
 * while a rename that COLLAPSES two fields into one still diverges — the
 * second field loses its own slot.
 *
 * Per-class, because private names are class-scoped: `#f` in class A and `#f`
 * in class B are unrelated, and the humanifier legitimately renames them to
 * different things.
 *
 * A partial rename cannot reach here: `#f` used without a declaration is a
 * SyntaxError, so anything that parsed has a complete one.
 */
function privateNameToken(name: string, state: SerializeState): string {
  if (!state.privateNamesAsSlots) return `P=#${name}`;
  if (!state.privateSlots) state.privateSlots = new Map();
  const slots = state.privateSlots;
  let slot = slots.get(name);
  if (slot === undefined) {
    slot = `P=$${slots.size + 1}`;
    slots.set(name, slot);
  }
  return slot;
}

function serializeValue(
  value: unknown,
  parent: t.Node | null,
  key: string,
  state: SerializeState
): void {
  if (value === null) {
    state.parts.push("null");
    return;
  }
  if (Array.isArray(value)) {
    state.parts.push("[");
    for (const item of value) {
      serializeValue(item, parent, key, state);
      state.parts.push(",");
    }
    state.parts.push("]");
    return;
  }
  if (isASTNode(value)) {
    serializeNode(value, parent, key, state);
    return;
  }
  state.parts.push(JSON.stringify(value) ?? String(value));
}

/**
 * The raw serialized token stream a path hashes to. Two same-hash paths
 * have identical streams; diffing the streams of two DIFFERENT-hash paths
 * pinpoints the first structurally-diverging token — the tool for
 * hash-instability root-causing (see experiments/013's
 * inspect-hash-divergence.ts) and shingle content comparison.
 * `preserveLiterals` matches the binding-fingerprint normalization
 * (`var a = 4` and `var a = 2` must differ when comparing binding content).
 */
export function serializePathTokens(
  path: NodePath,
  options?: SerializeOptions
): string[] {
  const bindingCache = bindingCacheFor(path);
  collectIdentifierBindings(path, bindingCache);
  const state: SerializeState = {
    parts: [],
    bindingByIdentifier: bindingCache,
    slotByDeclId: new Map(),
    bindingBySlot: new Map(),
    labelSlots: new Map(),
    mapping: new Map(),
    counter: 0,
    preserveLiterals: options?.preserveLiterals ?? false,
    privateNamesAsSlots: options?.privateNamesAsSlots,
    reExportLocalsVerbatim: options?.reExportLocalsVerbatim
  };
  serializeValue(path.node, null, "root", state);
  return state.parts;
}
