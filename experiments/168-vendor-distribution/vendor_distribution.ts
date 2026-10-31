/**
 * exp168 — where vendor (library) code goes, how its identifiers are named,
 * and what its cross-version churn is made of. READ-ONLY over one eval
 * workdir's existing trees; nothing runs the pipeline.
 *
 *   npx tsx experiments/168-vendor-distribution/vendor_distribution.ts \
 *     /tmp/eval-work/<label> [--version 2.1.216] [--json out.json]
 *   npx tsx .../vendor_distribution.ts --compare a.json b.json ... \
 *     [--bands r1.json r2.json r3.json]
 *
 * Every number is counted from what the run WROTE (rule 8: a metric can
 * only be wrong about what it looks at — the README lists what this does
 * not look at):
 *
 *  - the trees: `<wd>/<v>` (fresh) and the prior named by the run config;
 *  - the vendor record `vendor/_bun-modules.json` on both sides (which
 *    factory became which file, and the name ladder's label);
 *  - the run's `--diagnostics` file `<wd>/<v>.diag.json`: the placement
 *    trail (which file each APP statement went to, and which tier put it
 *    there), the strategy trail (who gave each app binding its final
 *    name), the library discovery's claims, and the era restoration's
 *    region claims (names mode only);
 *  - the raw input bundles (factory i of the record is the i-th top-level
 *    `var X = HELPER(function…)` of the bundle — validated per run below,
 *    never assumed).
 *
 * Vendor churn is the eval's own scorer (`decomposeVendorChurn`, the
 * card's `vendor` block), reproduced and then cut further by its per-file
 * rows; changed-line text comes from the one owner (`lib/diff.ts`).
 */
import fs from "node:fs";
import path from "node:path";
import * as parser from "@babel/parser";
import type { NodePath } from "@babel/traverse";
import * as t from "@babel/types";
import {
  parseFileAst,
  traverse,
  clearBabelTraverseCache
} from "../lib/js/babel.js";
import { changedLineTexts } from "../lib/diff.js";
import {
  decomposeVendorChurn,
  BUN_MANIFEST,
  type VendorFileRow
} from "../046-vendor-noise/vendor-churn.js";

// ───────────────────────────── inputs ─────────────────────────────

interface RunConfig {
  pair: string;
  version: string;
  label: string;
  resultsDir: string;
  input: string;
  prior: string;
  outputDir: string;
  configToml?: string;
}

interface ManifestEntry {
  fileName: string;
  name: string;
  nameSource: string;
  structuralHash: string;
  runtimeIdentifier: string;
  hashOrdinal?: number;
  identity?: { package?: string; source?: string; evidence?: string };
}

interface PlacementTrail {
  index: number;
  span: { start: number; end: number };
  names: string[];
  placedBy: string;
  file: string;
  priorFile?: string;
  priorFileFrom?: string;
}

interface StrategyTrail {
  oldName: string;
  declSpan: { start: number; end: number };
  declText: string;
  settledBy?: string;
  terminalBy?: string;
  finalName?: string;
}

interface Diag {
  placementTrails: { tiers: Record<string, number>; trails: PlacementTrail[] };
  strategyTrails: { trails: StrategyTrail[] };
  coverage: {
    mintedCensus?: { total: number; names: string[] };
  };
  libraryDiscovery?: {
    claims?: Array<{ start: number; end: number; package: string }>;
    restorations?: unknown[];
    restorationApplications?: Array<{
      start: number;
      end: number;
      package: string;
      slots: number[];
    }>;
  };
  eraRestoration?: {
    bodySpans?: Array<[number, number]>;
    regions?: Array<{ claims: Array<string | null>; statements: number[] }>;
    sites?: Array<{ applied: unknown[] }>;
    frozen?: unknown[];
  };
}

const readJson = <T>(p: string): T =>
  JSON.parse(fs.readFileSync(p, "utf8")) as T;

function argValue(args: string[], flag: string): string | undefined {
  const i = args.indexOf(flag);
  return i >= 0 ? args[i + 1] : undefined;
}

function argList(args: string[], flag: string): string[] {
  const i = args.indexOf(flag);
  if (i < 0) return [];
  const out: string[] = [];
  for (let j = i + 1; j < args.length && !args[j].startsWith("--"); j++)
    out.push(args[j]);
  return out;
}

function pickVersion(wd: string, wanted?: string): string {
  if (wanted) return wanted;
  const cfgs = fs
    .readdirSync(wd)
    .filter((f) => f.endsWith(".runcfg.json"))
    .map((f) => f.replace(/\.runcfg\.json$/, ""));
  if (cfgs.length !== 1)
    throw new Error(
      `${wd}: ${cfgs.length} run configs (${cfgs.join(", ")}) — pass --version`
    );
  return cfgs[0];
}

// ───────────────────────────── helpers ─────────────────────────────

/** `wc -l` lines: newline count, plus one for an unterminated last line. */
function textLines(s: string): number {
  if (s.length === 0) return 0;
  let n = 0;
  for (let i = 0; i < s.length; i++) if (s.charCodeAt(i) === 10) n++;
  return s.endsWith("\n") ? n : n + 1;
}

function walkFiles(root: string, skip: (rel: string) => boolean): string[] {
  const out: string[] = [];
  const walk = (dir: string) => {
    for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
      const full = path.join(dir, e.name);
      const rel = path.relative(root, full);
      if (skip(rel)) continue;
      if (e.isDirectory()) walk(full);
      else if (e.isFile()) out.push(rel);
    }
  };
  walk(root);
  return out.sort();
}

/** `vendor/<pkg>/…` → pkg (scoped: two segments); a root file → "(vendor root)". */
function packageOf(rel: string): string {
  const parts = rel.split("/").slice(1);
  if (parts.length <= 1) return "(vendor root)";
  if (parts[0].startsWith("@") && parts.length > 2)
    return `${parts[0]}/${parts[1]}`;
  return parts[0];
}

const fmt = (n: number) => n.toLocaleString("en-US");
const pct = (a: number, b: number) =>
  b === 0 ? "—" : `${((100 * a) / b).toFixed(1)}%`;

function table(head: string[], rows: Array<Array<string | number>>): string {
  const cells = rows.map((r) =>
    r.map((c) => (typeof c === "number" ? fmt(c) : c))
  );
  const line = (r: string[]) => `| ${r.join(" | ")} |`;
  return [
    line(head),
    line(head.map(() => "---")),
    ...cells.map((r) => line(r))
  ].join("\n");
}

// ───────────────────────── raw bundle factories ─────────────────────────

interface RawFactory {
  declStart: number;
  declEnd: number;
  fnText: string;
  /** Rename-invariant, relink-invariant shape of the factory function. */
  key: string;
}

const SKIP_KEYS = new Set([
  "type",
  "start",
  "end",
  "loc",
  "range",
  "extra",
  "comments",
  "leadingComments",
  "trailingComments",
  "innerComments"
]);

/**
 * The factory function's shape with every binding/reference name masked
 * and the relink's one rewrite undone: the emitted body reads a sibling
 * factory as `<relink import>.f()` where the bundle read `X()`, so a
 * member `R.f` on a relink import R reads as a plain identifier. Literals,
 * property keys and operators stay verbatim — so neither the minifier's
 * reroll nor the restoration's renames move it, and nothing else does.
 */
function shapeKey(fn: t.Node, relinkImports: Set<string>): string {
  const out: string[] = [];
  const visit = (node: unknown, propKey: boolean): void => {
    if (node === null || node === undefined) {
      out.push("_");
      return;
    }
    if (Array.isArray(node)) {
      out.push("[");
      for (const x of node) visit(x, false);
      out.push("]");
      return;
    }
    if (typeof node !== "object") {
      out.push(String(node));
      return;
    }
    const n = node as t.Node & Record<string, unknown>;
    switch (n.type) {
      case "Identifier":
        out.push(propKey ? `P:${n.name}` : "I");
        return;
      case "StringLiteral":
        out.push(`S:${JSON.stringify(n.value)}`);
        return;
      case "NumericLiteral":
        out.push(`N:${n.value}`);
        return;
      case "TemplateElement":
        out.push(`T:${JSON.stringify(n.value.raw)}`);
        return;
      case "MemberExpression":
        if (
          !n.computed &&
          n.object.type === "Identifier" &&
          relinkImports.has(n.object.name) &&
          n.property.type === "Identifier" &&
          n.property.name === "f"
        ) {
          out.push("I");
          return;
        }
        break;
    }
    out.push(`(${n.type}`);
    const isKeyed =
      n.type === "MemberExpression" ||
      n.type === "OptionalMemberExpression" ||
      n.type === "ObjectProperty" ||
      n.type === "ObjectMethod" ||
      n.type === "ClassMethod" ||
      n.type === "ClassProperty" ||
      n.type === "ClassPrivateProperty";
    for (const k of Object.keys(n)) {
      if (SKIP_KEYS.has(k)) continue;
      const v = n[k];
      const asKey = isKeyed && (k === "property" || k === "key") && !n.computed;
      if (
        typeof v === "boolean" ||
        typeof v === "string" ||
        typeof v === "number"
      ) {
        out.push(`${k}=${v}`);
        continue;
      }
      visit(v, asKey);
    }
    out.push(")");
  };
  visit(fn, false);
  return out.join(" ");
}

/**
 * The bundle's CommonJS factories in bundle order: every top-level
 * `var X = HELPER(function…)` whose HELPER occurs exactly `expected` times
 * (the vendor record's factory count). Refuses loudly when no helper does.
 */
function rawFactories(bundlePath: string, expected: number): RawFactory[] {
  const raw = fs.readFileSync(bundlePath, "utf8");
  const ast = parser.parse(raw, {
    sourceType: "script",
    errorRecovery: true,
    allowReturnOutsideFunction: true
  });
  let body: t.Statement[] = ast.program.body;
  for (const st of ast.program.body) {
    if (st.type !== "ExpressionStatement") continue;
    const e = st.expression;
    if (e.type === "FunctionExpression") body = e.body.body;
    else if (
      e.type === "CallExpression" &&
      e.callee.type === "FunctionExpression"
    )
      body = e.callee.body.body;
  }
  const byHelper = new Map<string, RawFactory[]>();
  for (const st of body) {
    if (st.type !== "VariableDeclaration") continue;
    for (const d of st.declarations) {
      const init = d.init;
      if (init?.type !== "CallExpression" || init.callee.type !== "Identifier")
        continue;
      const fn = init.arguments[0];
      if (
        !fn ||
        (fn.type !== "FunctionExpression" &&
          fn.type !== "ArrowFunctionExpression")
      )
        continue;
      const list = byHelper.get(init.callee.name) ?? [];
      list.push({
        declStart: d.start ?? 0,
        declEnd: d.end ?? 0,
        fnText: raw.slice(fn.start ?? 0, fn.end ?? 0),
        key: shapeKey(fn, new Set())
      });
      byHelper.set(init.callee.name, list);
    }
  }
  clearBabelTraverseCache();
  const hit = [...byHelper.entries()].filter(([, l]) => l.length === expected);
  if (hit.length !== 1)
    throw new Error(
      `${bundlePath}: ${hit.length} helpers declare exactly ${expected} factories ` +
        `(top: ${[...byHelper.entries()]
          .sort((a, b) => b[1].length - a[1].length)
          .slice(0, 4)
          .map(([k, l]) => `${k}×${l.length}`)
          .join(", ")})`
    );
  return hit[0][1];
}

// ───────────────────────── bindings in a function ─────────────────────────

interface Binding {
  name: string;
  /** Declared as `X = require("<bare package>")`. */
  bareRequire: boolean;
}

function isBareRequireInit(node: t.Node | null | undefined): boolean {
  return (
    !!node &&
    node.type === "CallExpression" &&
    node.callee.type === "Identifier" &&
    node.callee.name === "require" &&
    node.arguments[0]?.type === "StringLiteral" &&
    !node.arguments[0].value.startsWith(".")
  );
}

/** Every binding declared in `scopeRoot`'s subtree, in source order. */
function bindingsUnder(p: NodePath): Binding[] {
  const seen = new Map<number, Binding>();
  const add = (sp: NodePath) => {
    for (const [name, b] of Object.entries(sp.scope?.bindings ?? {})) {
      const start = b.identifier.start ?? -1;
      if (seen.has(start)) continue;
      const decl = b.path.node as t.Node;
      const bareRequire =
        decl.type === "VariableDeclarator" && isBareRequireInit(decl.init);
      seen.set(start, { name, bareRequire });
    }
  };
  if (p.isScopable()) add(p);
  p.traverse({ Scopable: (sp: NodePath) => add(sp) });
  return [...seen.entries()].sort((a, b) => a[0] - b[0]).map(([, b]) => b);
}

/** Bindings of a raw factory's function text. */
function rawFnBindings(fnText: string): Binding[] | null {
  const ast = parseFileAst(`(${fnText});`);
  if (!ast) return null;
  let out: Binding[] | null = null;
  traverse(ast, {
    Function(p: NodePath<t.Function>) {
      out = bindingsUnder(p);
      p.stop();
    }
  });
  clearBabelTraverseCache();
  return out;
}

/**
 * Bindings of an emitted factory file's factory function: the first
 * function argument of the call assigned to `exports.f` (the relink's
 * shape), else the file's first function argument of any call.
 */
function emittedFactory(
  code: string
): { bindings: Binding[]; key: string } | null {
  const ast = parseFileAst(code);
  if (!ast) return null;
  const relink = new Set<string>();
  for (const st of ast.program.body) {
    if (st.type !== "VariableDeclaration") continue;
    for (const d of st.declarations) {
      const init = d.init;
      if (
        d.id.type === "Identifier" &&
        init?.type === "CallExpression" &&
        init.callee.type === "Identifier" &&
        init.callee.name === "require" &&
        init.arguments[0]?.type === "StringLiteral" &&
        init.arguments[0].value.startsWith(".")
      )
        relink.add(d.id.name);
    }
  }
  let out: { bindings: Binding[]; key: string } | null = null;
  traverse(ast, {
    CallExpression(p: NodePath<t.CallExpression>) {
      const fn = p.get("arguments")[0];
      if (!fn || !(fn.isFunctionExpression() || fn.isArrowFunctionExpression()))
        return;
      out = { bindings: bindingsUnder(fn), key: shapeKey(fn.node, relink) };
      p.stop();
    }
  });
  clearBabelTraverseCache();
  return out;
}

/**
 * Pair each record entry with its raw factory BY SHAPE (the record's order
 * is not the bundle's — measured: it drifts from entry ~6 on 2.1.216).
 * Equal shapes pair in order. Returns entry index → raw index (or -1).
 */
function pairFactories(
  entries: ManifestEntry[],
  tree: string,
  raws: RawFactory[]
): {
  rawOf: number[];
  emitted: Array<{ bindings: Binding[]; key: string } | null>;
} {
  const byKey = new Map<string, number[]>();
  raws.forEach((r, i) => {
    const l = byKey.get(r.key) ?? [];
    l.push(i);
    byKey.set(r.key, l);
  });
  const rawOf: number[] = [];
  const emitted: Array<{ bindings: Binding[]; key: string } | null> = [];
  for (const e of entries) {
    const p = path.join(tree, e.fileName);
    const em =
      e.fileName.startsWith("vendor/") && fs.existsSync(p)
        ? emittedFactory(fs.readFileSync(p, "utf8"))
        : null;
    emitted.push(em);
    const l = em ? byKey.get(em.key) : undefined;
    rawOf.push(l && l.length > 0 ? (l.shift() as number) : -1);
  }
  return { rawOf, emitted };
}

/** Every binding of a whole file, in source order. */
function fileBindings(code: string): Binding[] | null {
  const ast = parseFileAst(code);
  if (!ast) return null;
  let out: Binding[] | null = null;
  traverse(ast, {
    Program(p: NodePath<t.Program>) {
      out = bindingsUnder(p);
      p.stop();
    }
  });
  clearBabelTraverseCache();
  return out;
}

/** String literals of a text, relative requires excluded (order-mapping check). */
function literalSet(code: string): Set<string> {
  const out = new Set<string>();
  for (const m of code.matchAll(/"((?:[^"\\\n]|\\.){4,80})"/g))
    if (!m[1].startsWith(".")) out.add(m[1]);
  return out;
}

function jaccard(a: Set<string>, b: Set<string>): number {
  if (a.size === 0 && b.size === 0) return 1;
  let inter = 0;
  for (const x of a) if (b.has(x)) inter++;
  return inter / (a.size + b.size - inter);
}

// ───────────────────────────── analysis ─────────────────────────────

type Mechanism =
  | "inherited from prior by hash (factory carry-over)"
  | "index claim moved by package"
  | "unclaimed-but-clean, moved on index owner+file (exp157)"
  | "whole-module factory, named this run (model/declared)"
  | "root lib_* fallback"
  | "era (app) statements filed into vendor by placement"
  | "the vendor record (_bun-modules.json)"
  | "other";

const MECHANISMS: Mechanism[] = [
  "inherited from prior by hash (factory carry-over)",
  "index claim moved by package",
  "unclaimed-but-clean, moved on index owner+file (exp157)",
  "whole-module factory, named this run (model/declared)",
  "root lib_* fallback",
  "era (app) statements filed into vendor by placement",
  "the vendor record (_bun-modules.json)",
  "other"
];

interface FileInfo {
  rel: string;
  lines: number;
  bytes: number;
  mechanism: Mechanism;
  /** Finer label (origin of a carried factory, tier of an era file…). */
  detail: string;
}

const EXACT = new Set([
  "exact-match",
  "statement-twin",
  "binding-cascade",
  "module-pin",
  "module-vote",
  "fn-name-vote",
  "fn-name-pin",
  "closure-capture"
]);
const MODEL = new Set(["llm", "retry", "module-naming", "coverage-sweep"]);

function eraSource(tr: StrategyTrail): string {
  if (tr.finalName === undefined || tr.finalName === tr.oldName)
    return "left as bundled (no final name, or kept the minifier's)";
  const by = tr.terminalBy ?? tr.settledBy ?? "(none)";
  if (EXACT.has(by)) return "carried from prior by exact match";
  if (by === "close-match") return "carried from prior by close match";
  if (MODEL.has(by)) return "model-named";
  if (by === "library-import") return "library-import name (finish rule)";
  if (by.startsWith("reconcile")) return "post-split reconcile (finish rule)";
  return `other (${by})`;
}

interface Analysis {
  label: string;
  version: string;
  pair: string;
  mode: string;
  status: Record<string, unknown>;
  tree: Record<string, { files: number; lines: number; bytes: number }>;
  vendorByMechanism: Record<
    string,
    { files: number; lines: number; bytes: number }
  >;
  vendorByDetail: Record<
    string,
    { files: number; lines: number; bytes: number }
  >;
  packages: Array<{
    package: string;
    files: number;
    lines: number;
    bytes: number;
    mechanism: string;
  }>;
  rootFiles: Array<{
    file: string;
    lines: number;
    bytes: number;
    mechanism: string;
  }>;
  checks: Record<string, unknown>;
  vendorNaming: Record<string, number>;
  vendorEraNaming: Record<string, number>;
  srcNaming: Record<string, number>;
  eraUnmapped: Record<string, number>;
  churn: Record<string, number>;
  churnMint: Record<string, number>;
  pathStability: Record<string, { files: number; lines: number }>;
  nameStability: Record<string, number>;
}

function priorVersionOf(pair: string): string {
  const [a] = pair.split("->");
  return a;
}

function runStatus(cfg: RunConfig, wd: string, version: string) {
  const status: Record<string, unknown> = {};
  const card = path.join(cfg.resultsDir, `${version}-run-status.json`);
  if (fs.existsSync(card)) {
    const s = readJson<{ exitCode: number; errors: unknown[] }>(card);
    status.runStatus = `exitCode ${s.exitCode}, errors ${s.errors.length}`;
  } else status.runStatus = `MISSING (${card} — card dir gone)`;
  const selfHop = path.join(cfg.resultsDir, `${version}-self-hop.json`);
  status.selfHopCard = fs.existsSync(selfHop) ? "present" : "missing";
  for (const d of [version, `${version}-selfhop`, `${version}-selfhop-warm`])
    status[`tree ${d}`] = fs.existsSync(path.join(wd, d, "index.js"))
      ? "present"
      : "missing";
  return status;
}

function analyze(
  wd: string,
  version: string,
  priorInputOverride?: string
): Analysis {
  const cfg = readJson<RunConfig>(path.join(wd, `${version}.runcfg.json`));
  const fresh = path.join(wd, version);
  const priorTree = path.dirname(path.dirname(cfg.prior));
  const priorV = priorVersionOf(cfg.pair);
  const priorInput =
    priorInputOverride ??
    cfg.input.replace(`claude-code-${version}`, `claude-code-${priorV}`);
  const mode = /matching\s*=\s*"names"/.test(cfg.configToml ?? "")
    ? "names"
    : "default";
  const checks: Record<string, unknown> = {};
  process.stderr.write(`[exp168] ${cfg.label} ${cfg.pair} (${mode})\n`);

  const status = runStatus(cfg, wd, version);
  const diag = readJson<Diag>(path.join(wd, `${version}.diag.json`));

  // ── 1. the tree ──
  const files = walkFiles(fresh, (rel) => rel === ".humanify");
  const tree: Analysis["tree"] = {};
  const info = new Map<
    string,
    { lines: number; bytes: number; text: string }
  >();
  for (const rel of files) {
    const text = fs.readFileSync(path.join(fresh, rel), "utf8");
    const area = rel.startsWith("src/")
      ? "src/"
      : rel.startsWith("vendor/")
        ? "vendor/"
        : "other";
    const lines = textLines(text);
    const bytes = Buffer.byteLength(text);
    info.set(rel, { lines, bytes, text });
    for (const k of [area, "TOTAL"]) {
      tree[k] ??= { files: 0, lines: 0, bytes: 0 };
      tree[k].files++;
      tree[k].lines += lines;
      tree[k].bytes += bytes;
    }
  }

  // ── vendor record, both sides ──
  const man = readJson<{ factories: ManifestEntry[] }>(
    path.join(fresh, "vendor", BUN_MANIFEST)
  ).factories;
  const priorMan = readJson<{ factories: ManifestEntry[] }>(
    path.join(priorTree, "vendor", BUN_MANIFEST)
  ).factories;
  const priorByFile = new Map(priorMan.map((e, i) => [e.fileName, { e, i }]));
  const hashKey = (e: ManifestEntry) =>
    `${e.structuralHash}#${e.hashOrdinal ?? 0}`;
  const priorByHash = new Map(priorMan.map((e) => [hashKey(e), e]));

  // ── raw factories, order-mapped to the record and validated ──
  const rawFresh = rawFactories(cfg.input, man.length);
  const rawPrior = fs.existsSync(priorInput)
    ? rawFactories(priorInput, priorMan.length)
    : null;
  const freshPair = pairFactories(man, fresh, rawFresh);
  const priorPair = rawPrior
    ? pairFactories(priorMan, priorTree, rawPrior)
    : null;
  const vendorEntries = man.filter((e) =>
    e.fileName.startsWith("vendor/")
  ).length;
  checks.factoryShapePairing = `${freshPair.rawOf.filter((x) => x >= 0).length}/${vendorEntries} fresh vendor factory files paired to a raw factory by shape (${priorPair ? `${priorPair.rawOf.filter((x) => x >= 0).length}/${priorMan.filter((e) => e.fileName.startsWith("vendor/")).length} prior` : "prior bundle missing"}); record order would have agreed for ${man.filter((_, i) => freshPair.rawOf[i] === i).length}`;

  // ── claims folded per fresh factory ──
  const claims = [...(diag.libraryDiscovery?.claims ?? [])].sort(
    (a, b) => a.start - b.start
  );
  const claimStarts = claims.map((c) => c.start);
  const firstAtOrAfter = (x: number) => {
    let lo = 0;
    let hi = claimStarts.length;
    while (lo < hi) {
      const mid = (lo + hi) >> 1;
      if (claimStarts[mid] < x) lo = mid + 1;
      else hi = mid;
    }
    return lo;
  };
  /** Entry index → folded claim of its paired raw factory ("?" = unpaired). */
  const factoryClaim: Array<string | null> = man.map((_, i) => {
    const ri = freshPair.rawOf[i];
    if (ri < 0) return "?";
    const f = rawFresh[ri];
    let pkg: string | null = null;
    for (
      let j = firstAtOrAfter(f.declStart);
      j < claims.length && claims[j].start < f.declEnd;
      j++
    ) {
      if (claims[j].end > f.declEnd) continue;
      if (pkg === null) pkg = claims[j].package;
      else if (pkg !== claims[j].package) pkg = "<contested>";
    }
    return pkg;
  });

  // ── placement: era statements in vendor ──
  const pt = diag.placementTrails.trails;
  const regionOfStmt = new Map<number, Array<string | null>>();
  for (const r of diag.eraRestoration?.regions ?? [])
    for (const s of r.statements) regionOfStmt.set(s, r.claims);
  const eraVendorFiles = new Map<string, Map<string, number>>();
  for (const tr of pt) {
    if (!tr.file.startsWith("vendor/")) continue;
    let label: string;
    if (tr.placedBy === "library-index") {
      const rc = regionOfStmt.get(tr.index);
      if (!rc) label = "library-index (region claims not recorded)";
      else {
        const pk = new Set(rc.filter((c) => c !== null));
        label =
          pk.size === 0
            ? "library-index:unclaimed"
            : pk.size === 1 && rc.every((c) => c !== null)
              ? "library-index:claimed"
              : "library-index:partial";
      }
    } else
      label = `${tr.placedBy}${tr.priorFileFrom === "hash" ? "+prior-path-by-hash" : ""}`;
    const m = eraVendorFiles.get(tr.file) ?? new Map<string, number>();
    m.set(label, (m.get(label) ?? 0) + 1);
    eraVendorFiles.set(tr.file, m);
  }

  // ── classify every vendor file ──
  const vendorFiles = files.filter((f) => f.startsWith("vendor/"));
  const manByFile = new Map(man.map((e, i) => [e.fileName, { e, i }]));
  const fileInfos: FileInfo[] = [];
  let carryCount = 0;
  for (const rel of vendorFiles) {
    const { lines, bytes } = info.get(rel) ?? { lines: 0, bytes: 0 };
    const m = manByFile.get(rel);
    let mechanism: Mechanism = "other";
    let detail = "";
    if (m) {
      const carried = priorByHash.get(hashKey(m.e));
      const inherited = carried !== undefined && carried.fileName === rel;
      const claim = factoryClaim[m.i];
      const indexClass =
        claim === null
          ? "unclaimed (exp157 rule)"
          : claim === "<contested>"
            ? "contested"
            : claim === "?"
              ? "claim unknown (factory not paired)"
              : "claimed";
      if (inherited) {
        carryCount++;
        mechanism = "inherited from prior by hash (factory carry-over)";
        detail = `carried; origin label ${m.e.nameSource}${m.e.nameSource === "index" ? `; this run's claims: ${indexClass}` : ""}`;
      } else if (m.e.nameSource === "index") {
        mechanism =
          claim === null
            ? "unclaimed-but-clean, moved on index owner+file (exp157)"
            : claim === "?"
              ? "other"
              : "index claim moved by package";
        detail = `index, decided this run, ${indexClass}`;
      } else if (
        m.e.nameSource === "fallback" ||
        /^vendor\/lib_[0-9a-f]+/.test(rel)
      ) {
        mechanism =
          rel.split("/").length === 2
            ? "root lib_* fallback"
            : "whole-module factory, named this run (model/declared)";
        detail = `${m.e.nameSource}, decided this run`;
      } else {
        mechanism = "whole-module factory, named this run (model/declared)";
        detail = `${m.e.nameSource}, decided this run`;
      }
    } else if (eraVendorFiles.has(rel)) {
      const m2 = eraVendorFiles.get(rel) ?? new Map();
      const top = [...m2.entries()].sort((a, b) => b[1] - a[1]);
      const lead = top[0][0];
      mechanism = lead.startsWith("library-index:claimed")
        ? "index claim moved by package"
        : lead.startsWith("library-index:unclaimed")
          ? "unclaimed-but-clean, moved on index owner+file (exp157)"
          : lead.startsWith("library-index")
            ? "index claim moved by package"
            : "era (app) statements filed into vendor by placement";
      detail = `era file: ${top.map(([k, v]) => `${k}×${v}`).join(", ")}`;
    } else if (rel === `vendor/${BUN_MANIFEST}`) {
      mechanism = "the vendor record (_bun-modules.json)";
      detail = "humanify's own metadata, not library code";
    } else detail = "not in the record, no placement trail";
    fileInfos.push({ rel, lines, bytes, mechanism, detail });
  }
  checks.carryOverReconstructed = `${carryCount} factories inherited by (structural hash, ordinal) at the same path (the run log's "carry-over" count is the check where a log exists)`;
  checks.eraVendorStatements = [...eraVendorFiles.values()].reduce(
    (n, m) => n + [...m.values()].reduce((a, b) => a + b, 0),
    0
  );
  checks.factoriesWithContestedClaim = factoryClaim.filter(
    (c) => c === "<contested>"
  ).length;
  checks.indexLabelledFreshFactories = man.filter(
    (e) => e.nameSource === "index"
  ).length;

  const add = (
    acc: Record<string, { files: number; lines: number; bytes: number }>,
    k: string,
    f: FileInfo
  ) => {
    acc[k] ??= { files: 0, lines: 0, bytes: 0 };
    acc[k].files++;
    acc[k].lines += f.lines;
    acc[k].bytes += f.bytes;
  };
  const vendorByMechanism: Analysis["vendorByMechanism"] = {};
  const vendorByDetail: Analysis["vendorByDetail"] = {};
  for (const f of fileInfos) {
    add(vendorByMechanism, f.mechanism, f);
    add(vendorByMechanism, "TOTAL", f);
    add(
      vendorByDetail,
      `${f.mechanism} :: ${f.detail.replace(/×\d+/g, "").replace(/era file: .*/, (d) => [...new Set(d.split(", "))].sort().join(", "))}`,
      f
    );
  }

  // per package
  const pk = new Map<
    string,
    { files: number; lines: number; bytes: number; mech: Map<string, number> }
  >();
  for (const f of fileInfos) {
    const p = packageOf(f.rel);
    const e = pk.get(p) ?? { files: 0, lines: 0, bytes: 0, mech: new Map() };
    e.files++;
    e.lines += f.lines;
    e.bytes += f.bytes;
    e.mech.set(f.mechanism, (e.mech.get(f.mechanism) ?? 0) + f.bytes);
    pk.set(p, e);
  }
  const packages = [...pk.entries()]
    .map(([p, e]) => ({
      package: p,
      files: e.files,
      lines: e.lines,
      bytes: e.bytes,
      mechanism: [...e.mech.entries()].sort((a, b) => b[1] - a[1])[0][0]
    }))
    .sort((a, b) => b.bytes - a.bytes);
  const rootFiles = fileInfos
    .filter((f) => f.rel.split("/").length === 2)
    .map((f) => ({
      file: f.rel,
      lines: f.lines,
      bytes: f.bytes,
      mechanism: f.mechanism
    }));

  // ── 2. naming inside vendor factory files ──
  const vendorNaming: Record<string, number> = {};
  const bump = (acc: Record<string, number>, k: string, n = 1) => {
    acc[k] = (acc[k] ?? 0) + n;
  };
  const priorText = (rel: string) => {
    const p = path.join(priorTree, rel);
    return fs.existsSync(p) ? fs.readFileSync(p, "utf8") : null;
  };
  const apps = [...(diag.libraryDiscovery?.restorationApplications ?? [])].sort(
    (a, b) => a.start - b.start
  );
  const appStarts = apps.map((a) => a.start);
  const appsInFast = (f: RawFactory) => {
    let lo = 0;
    let hi = appStarts.length;
    while (lo < hi) {
      const mid = (lo + hi) >> 1;
      if (appStarts[mid] < f.declStart) lo = mid + 1;
      else hi = mid;
    }
    let n = 0;
    for (let j = lo; j < apps.length && apps[j].start < f.declEnd; j++)
      if (apps[j].end <= f.declEnd) n += apps[j].slots.length;
    return n;
  };
  let slotsInFactories = 0;
  for (let i = 0; i < man.length; i++) {
    const e = man[i];
    const text = info.get(e.fileName)?.text;
    if (!text || !e.fileName.startsWith("vendor/")) continue;
    const ri = freshPair.rawOf[i];
    const got = freshPair.emitted[i]?.bindings ?? null;
    if (ri < 0 || !got) {
      bump(
        vendorNaming,
        "factory not paired to the raw bundle",
        got?.length ?? 0
      );
      continue;
    }
    const restoredHere = appsInFast(rawFresh[ri]);
    slotsInFactories += restoredHere;
    const raw = rawFnBindings(rawFresh[ri].fnText);
    if (!raw || got.length !== raw.length) {
      bump(vendorNaming, "not zippable (binding counts differ)", got.length);
      continue;
    }
    const pt0 = priorText(e.fileName);
    const inherited = pt0 !== null && pt0 === text;
    let priorRaw: Binding[] | null = null;
    if (inherited && rawPrior && priorPair) {
      const pi = priorByFile.get(e.fileName);
      const pri = pi ? priorPair.rawOf[pi.i] : -1;
      if (pri >= 0) priorRaw = rawFnBindings(rawPrior[pri].fnText);
    }
    for (let k = 0; k < got.length; k++) {
      const g = got[k];
      let label: string;
      if (g.name === raw[k].name) label = "left as bundled (minifier's name)";
      else if (inherited) {
        if (!priorRaw || priorRaw.length !== got.length)
          label =
            "carried from prior: file inherited (prior side not zippable)";
        else if (priorRaw[k].name === g.name)
          label =
            "carried from prior: file inherited, still the prior's minifier name";
        else if (restoredHere > 0)
          label =
            "restored from the library index (file answered this run; bytes inherited from prior)";
        else
          label =
            "carried from prior: file inherited, a name the prior run gave";
      } else if (g.bareRequire) label = "library-import name (finish rule)";
      else if (restoredHere > 0)
        label = "restored from the library index (vendored body)";
      else label = "renamed this run, source unattributed";
      bump(vendorNaming, label);
    }
  }
  checks.restorationApplicationSlots = apps.reduce(
    (n, a) => n + a.slots.length,
    0
  );
  checks.restorationApplicationSlotsInFactories = slotsInFactories;

  // ── 2b. naming of era (app) bindings, by the file their statement went to ──
  const srcNaming: Record<string, number> = {};
  const vendorEraNaming: Record<string, number> = {};
  const eraUnmapped: Record<string, number> = {};
  const bodySpans = diag.eraRestoration?.bodySpans ?? null;
  const pStarts = pt.map((p) => p.span.start);
  const freshStarts = bodySpans?.map((s) => s[0]) ?? null;
  const find = (starts: number[], ends: (i: number) => number, x: number) => {
    let lo = 0;
    let hi = starts.length - 1;
    let ans = -1;
    while (lo <= hi) {
      const mid = (lo + hi) >> 1;
      if (starts[mid] <= x) {
        ans = mid;
        lo = mid + 1;
      } else hi = mid - 1;
    }
    return ans >= 0 && x < ends(ans) ? ans : -1;
  };
  const tokensOf = new Map<string, Set<string>>();
  const fileHasToken = (rel: string, name: string) => {
    let set = tokensOf.get(rel);
    if (!set) {
      set = new Set(info.get(rel)?.text.match(/[A-Za-z_$][\w$]*/g) ?? []);
      tokensOf.set(rel, set);
    }
    return set.has(name);
  };
  /** Without bodySpans a fresh-text trail has no file — unless the run put no app statement in vendor/. */
  const noEraInVendor = eraVendorFiles.size === 0;
  let mappedNamed = 0;
  let mappedNamedInFile = 0;
  let assumedSrc = 0;
  for (const tr of diag.strategyTrails.trails) {
    const src = eraSource(tr);
    let file: string | null = null;
    let idx = -1;
    if (tr.declText === "fresh" && freshStarts && bodySpans)
      idx = find(freshStarts, (i) => bodySpans[i][1], tr.declSpan.start);
    else if (tr.declText === "generated")
      idx = find(pStarts, (i) => pt[i].span.end, tr.declSpan.start);
    else if (tr.declText.includes("/")) file = tr.declText;
    if (idx >= 0) file = pt[idx].file;
    if (file !== null && tr.finalName) {
      mappedNamed++;
      if (fileHasToken(file, tr.finalName)) mappedNamedInFile++;
    }
    if (
      file === null &&
      tr.declText === "fresh" &&
      !freshStarts &&
      noEraInVendor
    ) {
      file = "src/(assumed: no app statement went to vendor/)";
      assumedSrc++;
    }
    if (file === null)
      bump(
        eraUnmapped,
        `${src} [${tr.declText === "fresh" ? "fresh text, no bodySpans" : tr.declText}]`
      );
    else if (file.startsWith("vendor/")) bump(vendorEraNaming, src);
    else bump(srcNaming, src);
  }
  checks.eraTrailFileMapping = `${mappedNamedInFile}/${mappedNamed} trails mapped to a file find their final name in that file's text (a wrong coordinate system reads far lower); ${assumedSrc} fresh-text trails assigned to src/ without bodySpans because no app statement went to vendor/`;
  checks.eraRestorationSiteAApplied = (diag.eraRestoration?.sites ?? []).reduce(
    (n, s) => n + s.applied.length,
    0
  );
  checks.eraRestorationFrozen = diag.eraRestoration?.frozen?.length ?? null;
  checks.mintedCensusTotal = diag.coverage.mintedCensus?.total ?? null;

  // ── 3. vendor churn (the eval's scorer, then cut by its rows) ──
  const v = decomposeVendorChurn(
    path.join(priorTree, "vendor"),
    path.join(fresh, "vendor")
  );
  const b = v.bodies;
  const churn: Record<string, number> = {
    churnLines: v.vendorTotalLines,
    "noise (vendorLn)": v.noiseLines,
    "real (vendorReal)": v.realDependencyChangeLines,
    manifest: v.manifest.changedLines,
    "name re-rolls (nameOnly+freeReroll)":
      b.nameOnly.lines + b.freeReroll.lines,
    "moved / relocated (movedPath+relocated)":
      b.movedPath.lines + b.relocated.lines,
    "real: changed in place or paired": b.realChange.lines,
    "real: truly added": b.trulyAdded.lines,
    "real: truly removed": b.trulyRemoved.lines,
    "other (unparsed)": b.unparsed.lines
  };
  // mint-bearing lines: a changed line holding a census mint that the file declares
  const census = new Set(diag.coverage.mintedCensus?.names ?? []);
  const churnMint: Record<string, number> = {};
  const isEraFile = (rel?: string) =>
    !!rel && eraVendorFiles.has(`vendor/${rel}`);
  const priorVendor = path.join(priorTree, "vendor");
  const freshVendor = path.join(fresh, "vendor");
  const readOr = (dir: string, rel?: string) =>
    rel && fs.existsSync(path.join(dir, rel))
      ? fs.readFileSync(path.join(dir, rel), "utf8")
      : "";
  for (const row of v.fileRows) {
    if (row.lines === 0) continue;
    if (!isEraFile(row.fresh) && !isEraFile(row.prior)) continue;
    const pt1 = readOr(priorVendor, row.prior);
    const ft1 = readOr(freshVendor, row.fresh);
    const declared = new Set(
      [...(fileBindings(ft1) ?? []), ...(fileBindings(pt1) ?? [])].map(
        (x) => x.name
      )
    );
    const mints = [...declared].filter((n) => census.has(n));
    if (mints.length === 0) continue;
    const re = new RegExp(
      `(^|[^A-Za-z0-9_$])(${mints.map((m) => m.replace(/\$/g, "\\$")).join("|")})(?![A-Za-z0-9_$])`
    );
    const texts =
      row.bucket === "trulyAdded"
        ? ft1.split("\n")
        : row.bucket === "trulyRemoved"
          ? pt1.split("\n")
          : changedLineTexts(pt1, ft1);
    const hits = texts.filter((l) => re.test(l)).length;
    bump(churnMint, bucketGroup(row), Math.min(hits, row.lines));
  }

  // path stability
  const pathStability: Analysis["pathStability"] = {};
  const ps = (k: string, n: number) => {
    pathStability[k] ??= { files: 0, lines: 0 };
    pathStability[k].files++;
    pathStability[k].lines += n;
  };
  for (const row of v.fileRows) {
    if (row.bucket === "identical") ps("kept path, byte-identical", 0);
    else if (row.bucket === "trulyAdded")
      ps("new (content absent from the prior)", row.lines);
    else if (row.bucket === "trulyRemoved")
      ps("removed (content absent from the fresh)", row.lines);
    else if (row.bucket === "unparsed") ps("unparsed", row.lines);
    else if (row.bucket === "relocated")
      ps("moved and changed a little (relocation pair, path draw)", row.lines);
    else if (row.prior === row.fresh) ps(`kept path, ${row.bucket}`, row.lines);
    else ps(`moved, ${row.bucket}`, row.lines);
  }

  // identifier-name stability for identical code (same content signature)
  const nameStability: Record<string, number> = {};
  for (const row of v.fileRows) {
    if (!["identical", "nameOnly", "movedPath"].includes(row.bucket)) continue;
    if (row.bucket === "movedPath" && row.prior === undefined) continue;
    const ft1 = readOr(freshVendor, row.fresh);
    const fb = fileBindings(ft1);
    if (!fb) continue;
    const kind = isEraFile(row.fresh) ? "era file" : "factory file";
    if (row.bucket === "identical") {
      bump(nameStability, `${kind}: same name`, fb.length);
      continue;
    }
    const pb = fileBindings(readOr(priorVendor, row.prior));
    if (!pb || pb.length !== fb.length) {
      bump(nameStability, `${kind}: not zippable`, fb.length);
      continue;
    }
    for (let k = 0; k < fb.length; k++)
      bump(
        nameStability,
        `${kind}: ${fb[k].name === pb[k].name ? "same name" : "different name"}`
      );
  }

  return {
    label: cfg.label,
    version,
    pair: cfg.pair,
    mode,
    status,
    tree,
    vendorByMechanism,
    vendorByDetail,
    packages,
    rootFiles,
    checks,
    vendorNaming,
    vendorEraNaming,
    srcNaming,
    eraUnmapped,
    churn,
    churnMint,
    pathStability,
    nameStability
  };
}

function bucketGroup(row: VendorFileRow): string {
  switch (row.bucket) {
    case "nameOnly":
    case "freeReroll":
      return "name re-rolls (nameOnly+freeReroll)";
    case "movedPath":
    case "relocated":
      return "moved / relocated (movedPath+relocated)";
    case "realChange":
      return "real: changed in place or paired";
    case "trulyAdded":
      return "real: truly added";
    case "trulyRemoved":
      return "real: truly removed";
    default:
      return "other (unparsed)";
  }
}

// ───────────────────────────── printing ─────────────────────────────

function sumOf(r: Record<string, number>): number {
  return Object.values(r).reduce((a, b) => a + b, 0);
}

function printNaming(title: string, r: Record<string, number>): string {
  const total = sumOf(r);
  const rows = Object.entries(r)
    .sort((a, b) => b[1] - a[1])
    .map(([k, n]) => [k, n, pct(n, total)]);
  return `${title}\n\n${table(["name source", "bindings", "share"], [["TOTAL", total, "100%"], ...rows])}`;
}

function report(a: Analysis): string {
  const out: string[] = [];
  out.push(`# ${a.label} — ${a.pair} (${a.mode} mode)\n`);
  out.push("## Run status\n");
  out.push(
    table(
      ["check", "value"],
      Object.entries(a.status).map(([k, v]) => [k, String(v)])
    )
  );
  out.push("\n## Consistency checks (each counter shown able to fire)\n");
  out.push(
    table(
      ["check", "value"],
      Object.entries(a.checks).map(([k, v]) => [k, String(v)])
    )
  );

  out.push("\n## 1. Where code goes (the fresh tree, .humanify/ excluded)\n");
  const T = a.tree.TOTAL;
  out.push(
    table(
      ["area", "files", "lines", "bytes", "share of lines"],
      ["TOTAL", "src/", "vendor/", "other"].map((k) => [
        k,
        a.tree[k]?.files ?? 0,
        a.tree[k]?.lines ?? 0,
        a.tree[k]?.bytes ?? 0,
        pct(a.tree[k]?.lines ?? 0, T.lines)
      ])
    )
  );
  out.push(
    "\n### vendor/ by the mechanism that placed each file (exclusive)\n"
  );
  const V = a.vendorByMechanism.TOTAL;
  out.push(
    table(
      ["mechanism", "files", "lines", "bytes", "share of vendor bytes"],
      [
        ["TOTAL", V.files, V.lines, V.bytes, "100%"],
        ...MECHANISMS.map((m) => {
          const e = a.vendorByMechanism[m] ?? { files: 0, lines: 0, bytes: 0 };
          return [m, e.files, e.lines, e.bytes, pct(e.bytes, V.bytes)];
        })
      ]
    )
  );
  out.push("\n### finer detail (mechanism :: label)\n");
  out.push(
    table(
      ["mechanism :: detail", "files", "lines", "bytes"],
      Object.entries(a.vendorByDetail)
        .sort((x, y) => y[1].bytes - x[1].bytes)
        .map(([k, e]) => [k, e.files, e.lines, e.bytes])
    )
  );
  out.push("\n### per package (top 25 by bytes, then the long tail)\n");
  const top = a.packages.slice(0, 25);
  const tail = a.packages.slice(25);
  const tailSum = tail.reduce(
    (s, p) => ({
      files: s.files + p.files,
      lines: s.lines + p.lines,
      bytes: s.bytes + p.bytes
    }),
    { files: 0, lines: 0, bytes: 0 }
  );
  out.push(
    table(
      [
        "package",
        "files",
        "lines",
        "bytes",
        "share of bytes",
        "dominant mechanism (by bytes)"
      ],
      [
        ...top.map((p) => [
          p.package,
          p.files,
          p.lines,
          p.bytes,
          pct(p.bytes, V.bytes),
          p.mechanism
        ]),
        [
          `long tail (${tail.length} packages)`,
          tailSum.files,
          tailSum.lines,
          tailSum.bytes,
          pct(tailSum.bytes, V.bytes),
          "—"
        ]
      ]
    )
  );
  out.push("\n### vendor-root loose files\n");
  const rootByMech = new Map<
    string,
    { files: number; lines: number; bytes: number }
  >();
  for (const f of a.rootFiles) {
    const e = rootByMech.get(f.mechanism) ?? { files: 0, lines: 0, bytes: 0 };
    e.files++;
    e.lines += f.lines;
    e.bytes += f.bytes;
    rootByMech.set(f.mechanism, e);
  }
  out.push(
    table(
      ["mechanism", "files", "lines", "bytes"],
      [...rootByMech.entries()].map(([k, e]) => [k, e.files, e.lines, e.bytes])
    )
  );

  out.push("\n## 2. How identifiers are named\n");
  out.push(
    printNaming(
      "### vendor/ factory bodies (each binding zipped against the raw bundle's factory)",
      a.vendorNaming
    )
  );
  out.push("");
  out.push(
    printNaming(
      "### vendor/ era (app) statements (strategy trail, by terminal step)",
      a.vendorEraNaming
    )
  );
  out.push("");
  out.push(
    printNaming(
      "### src/ (strategy trail, by terminal step) — the contrast",
      a.srcNaming
    )
  );
  if (sumOf(a.eraUnmapped) > 0) {
    out.push("");
    out.push(
      printNaming(
        "### app bindings whose file could not be mapped",
        a.eraUnmapped
      )
    );
  }

  out.push("\n## 3. vendor/ churn, prior → fresh (the eval's vendor scorer)\n");
  const c = a.churn;
  out.push(
    table(
      ["bucket", "lines", "of which mint-bearing"],
      [
        ["TOTAL churnLines", c.churnLines, sumOf(a.churnMint)],
        ["manifest", c.manifest, 0],
        [
          "name re-rolls (nameOnly+freeReroll)",
          c["name re-rolls (nameOnly+freeReroll)"],
          a.churnMint["name re-rolls (nameOnly+freeReroll)"] ?? 0
        ],
        [
          "moved / relocated (movedPath+relocated)",
          c["moved / relocated (movedPath+relocated)"],
          a.churnMint["moved / relocated (movedPath+relocated)"] ?? 0
        ],
        [
          "real: changed in place or paired",
          c["real: changed in place or paired"],
          a.churnMint["real: changed in place or paired"] ?? 0
        ],
        [
          "real: truly added",
          c["real: truly added"],
          a.churnMint["real: truly added"] ?? 0
        ],
        [
          "real: truly removed",
          c["real: truly removed"],
          a.churnMint["real: truly removed"] ?? 0
        ],
        [
          "other (unparsed)",
          c["other (unparsed)"],
          a.churnMint["other (unparsed)"] ?? 0
        ],
        ["= noise (vendorLn)", c["noise (vendorLn)"], ""],
        ["= real (vendorReal)", c["real (vendorReal)"], ""]
      ]
    )
  );
  out.push("\n### file-path stability (vendor files)\n");
  out.push(
    table(
      ["outcome", "files", "lines charged"],
      Object.entries(a.pathStability)
        .sort((x, y) => y[1].files - x[1].files)
        .map(([k, e]) => [k, e.files, e.lines])
    )
  );
  out.push(
    "\n### identifier-name stability for identical code (same content signature)\n"
  );
  out.push(
    table(
      ["file kind: outcome", "bindings"],
      Object.entries(a.nameStability)
        .sort()
        .map(([k, n]) => [k, n])
    )
  );
  return out.join("\n");
}

// ───────────────────────────── comparison ─────────────────────────────

function flatten(a: Analysis): Record<string, number> {
  const out: Record<string, number> = {};
  for (const k of ["TOTAL", "src/", "vendor/", "other"]) {
    out[`tree ${k} lines`] = a.tree[k]?.lines ?? 0;
    out[`tree ${k} bytes`] = a.tree[k]?.bytes ?? 0;
  }
  for (const m of MECHANISMS)
    out[`vendor: ${m} (bytes)`] = a.vendorByMechanism[m]?.bytes ?? 0;
  for (const m of MECHANISMS)
    out[`vendor: ${m} (files)`] = a.vendorByMechanism[m]?.files ?? 0;
  for (const [k, n] of Object.entries(a.vendorNaming))
    out[`vendor body names: ${k}`] = n;
  for (const [k, n] of Object.entries(a.vendorEraNaming))
    out[`vendor era names: ${k}`] = n;
  for (const [k, n] of Object.entries(a.srcNaming)) out[`src names: ${k}`] = n;
  // every app binding whatever file it went to (exp161's diags predate bodySpans)
  for (const r of [a.srcNaming, a.vendorEraNaming, a.eraUnmapped])
    for (const [k, n] of Object.entries(r)) {
      const key = `app names, all files: ${k.replace(/ \[[^\]]*\]$/, "")}`;
      out[key] = (out[key] ?? 0) + n;
    }
  for (const [k, n] of Object.entries(a.churn)) out[`churn: ${k}`] = n;
  out["churn: mint-bearing (all buckets)"] = sumOf(a.churnMint);
  for (const [k, e] of Object.entries(a.pathStability))
    out[`path: ${k} (files)`] = e.files;
  for (const [k, n] of Object.entries(a.nameStability))
    out[`identical-code names: ${k}`] = n;
  return out;
}

function compare(files: string[], bandFiles: string[]): string {
  const runs = files.map((f) => flatten(readJson<Analysis>(f)));
  const labels = files.map((f) => readJson<Analysis>(f).label);
  const bandRuns = bandFiles.map((f) => flatten(readJson<Analysis>(f)));
  const keys = [...new Set(runs.flatMap((r) => Object.keys(r)))];
  const head = [
    "metric",
    ...labels,
    ...labels.slice(1).map((l) => `Δ ${l} − ${labels[0]}`),
    "band (3 refs, max−min)"
  ];
  const rows = keys.map((k) => {
    const vals = runs.map((r) => r[k] ?? 0);
    let band: number | null = null;
    if (bandRuns.length >= 2) {
      const bv = bandRuns.map((r) => r[k] ?? 0);
      band = Math.max(...bv) - Math.min(...bv);
    }
    const deltas = vals.slice(1).map((v) => {
      const d = v - vals[0];
      if (band !== null && Math.abs(d) <= band) return `~0 (±${fmt(band)})`;
      return `${d > 0 ? "+" : ""}${fmt(d)}`;
    });
    return [
      k,
      ...vals.map(fmt),
      ...deltas,
      band === null ? "—" : `±${fmt(band)}`
    ];
  });
  return table(head, rows);
}

// ───────────────────────────── main ─────────────────────────────

function main() {
  const args = process.argv.slice(2);
  if (args[0] === "--compare") {
    const files = argList(args, "--compare");
    console.log(compare(files, argList(args, "--bands")));
    return;
  }
  const wd = args[0];
  if (!wd || wd.startsWith("--"))
    throw new Error(
      "usage: vendor_distribution.ts <workdir> [--version V] [--json out] | --compare a.json b.json … [--bands r1.json …]"
    );
  const version = pickVersion(wd, argValue(args, "--version"));
  const a = analyze(wd, version, argValue(args, "--prior-input"));
  const json = argValue(args, "--json");
  if (json) fs.writeFileSync(json, `${JSON.stringify(a, null, 1)}\n`);
  console.log(report(a));
}

main();
