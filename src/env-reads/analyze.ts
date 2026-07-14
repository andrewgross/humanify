/**
 * Static inventory of environment-variable reads in bundled/minified JS.
 *
 * Walks `process.env` / `Bun.env` / `import.meta.env` bases and classifies
 * each use by inspecting its parent: a member access (`env.FOO`,
 * `env["FOO"]`) resolves to a variable name; a computed dynamic key
 * (`env[x]`) is flagged as unresolvable; a destructure (`const {A} = env`)
 * yields its keys; an alias (`const e = env`) is followed through its
 * references; anything else (passing the whole object, `Object.keys(env)`,
 * `{...env}`) is an enumerated / whole-env use.
 *
 * The dynamic-key reads are inherently unresolvable statically — they are
 * surfaced with locations rather than hidden, so the report never pretends
 * to completeness.
 */

import type { NodePath } from "@babel/core";
import type { Scope } from "@babel/traverse";
import * as t from "@babel/types";
import { parseFileAst, traverse } from "../babel-utils.js";

export interface EnvLocation {
  file: string;
  line: number;
  column: number;
}

export interface EnvVarReads {
  name: string;
  locations: EnvLocation[];
}

export interface EnvSiteUse {
  loc: EnvLocation;
  snippet: string;
}

export interface EnvReadsReport {
  /** Resolved variable names, sorted, each with every read site. */
  byVar: EnvVarReads[];
  /** Computed dynamic keys — not statically resolvable. */
  dynamic: EnvSiteUse[];
  /** Whole-env / enumerated uses (spread, Object.keys, passed as a value). */
  enumerated: EnvSiteUse[];
  filesAnalyzed: number;
}

type Finding =
  | { kind: "var"; name: string; loc: EnvLocation }
  | { kind: "dynamic"; loc: EnvLocation; snippet: string }
  | { kind: "enumerated"; loc: EnvLocation; snippet: string };

interface Ctx {
  file: string;
  code: string;
  findings: Finding[];
  seen: Set<t.Node>;
}

function locOf(node: t.Node, file: string): EnvLocation {
  return {
    file,
    line: node.loc?.start.line ?? 0,
    column: node.loc?.start.column ?? 0
  };
}

/** A short, single-line source excerpt for a node, for the dynamic/enumerated
 * sections where a name alone is uninformative. */
function snippetOf(node: t.Node, code: string): string {
  if (node.start == null || node.end == null) return "";
  const end = Math.min(node.end, node.start + 80);
  return code.slice(node.start, end).replace(/\s+/g, " ").trim();
}

/** True when `node` is a `process.env` / `Bun.env` / `import.meta.env` base. */
function isEnvBase(node: t.Node): node is t.MemberExpression {
  if (!t.isMemberExpression(node) || node.computed) return false;
  if (!t.isIdentifier(node.property, { name: "env" })) return false;
  const obj = node.object;
  if (t.isIdentifier(obj, { name: "process" })) return true;
  if (t.isIdentifier(obj, { name: "Bun" })) return true;
  return (
    t.isMetaProperty(obj) &&
    obj.meta.name === "import" &&
    obj.property.name === "meta"
  );
}

/** A `process`/`Bun` base that resolves to a LOCAL binding is a shadow of the
 * global, not the real environment — skip it. (import.meta cannot be shadowed.) */
function isShadowed(node: t.MemberExpression, scope: Scope): boolean {
  return (
    t.isIdentifier(node.object) && scope.getBinding(node.object.name) != null
  );
}

function enumeratedFinding(node: t.Node, ctx: Ctx): Finding {
  return {
    kind: "enumerated",
    loc: locOf(node, ctx.file),
    snippet: snippetOf(node, ctx.code)
  };
}

/** Resolve a member access on the env object to a variable name (or a dynamic
 * finding when the key is computed at runtime). */
function classifyMember(member: NodePath<t.MemberExpression>, ctx: Ctx): void {
  const node = member.node;
  const key = node.property;
  if (!node.computed && t.isIdentifier(key)) {
    ctx.findings.push({
      kind: "var",
      name: key.name,
      loc: locOf(node, ctx.file)
    });
  } else if (t.isStringLiteral(key)) {
    ctx.findings.push({
      kind: "var",
      name: key.value,
      loc: locOf(node, ctx.file)
    });
  } else {
    ctx.findings.push({
      kind: "dynamic",
      loc: locOf(node, ctx.file),
      snippet: snippetOf(node, ctx.code)
    });
  }
}

/** Resolve a single ObjectPattern property (`const { KEY } = env`). */
function classifyPatternProp(
  prop: t.ObjectProperty | t.RestElement,
  ctx: Ctx
): void {
  if (t.isRestElement(prop)) {
    ctx.findings.push(enumeratedFinding(prop, ctx));
    return;
  }
  const key = prop.key;
  if (!prop.computed && t.isIdentifier(key)) {
    ctx.findings.push({
      kind: "var",
      name: key.name,
      loc: locOf(prop, ctx.file)
    });
  } else if (t.isStringLiteral(key)) {
    ctx.findings.push({
      kind: "var",
      name: key.value,
      loc: locOf(prop, ctx.file)
    });
  } else {
    ctx.findings.push({
      kind: "dynamic",
      loc: locOf(prop, ctx.file),
      snippet: snippetOf(prop, ctx.code)
    });
  }
}

/** Follow an aliased env binding (`const e = env`) to each of its references. */
function classifyAlias(
  id: t.Identifier,
  decl: NodePath<t.VariableDeclarator>,
  ctx: Ctx
): void {
  const binding = decl.scope.getBinding(id.name);
  if (!binding) return;
  for (const ref of binding.referencePaths) classifyUse(ref, ctx);
}

/** Classify a `const … = env` declarator: destructure, alias, or (odd) whole. */
function classifyDeclarator(
  decl: NodePath<t.VariableDeclarator>,
  ctx: Ctx
): void {
  const id = decl.node.id;
  if (t.isObjectPattern(id)) {
    for (const prop of id.properties) classifyPatternProp(prop, ctx);
  } else if (t.isIdentifier(id)) {
    classifyAlias(id, decl, ctx);
  } else if (decl.node.init) {
    ctx.findings.push(enumeratedFinding(decl.node.init, ctx));
  }
}

/** Classify one env-object occurrence (a base, or an alias reference). */
function classifyUse(path: NodePath, ctx: Ctx): void {
  if (ctx.seen.has(path.node)) return;
  ctx.seen.add(path.node);
  const parent = path.parentPath;
  if (parent?.isMemberExpression() && parent.node.object === path.node) {
    classifyMember(parent, ctx);
  } else if (parent?.isVariableDeclarator() && parent.node.init === path.node) {
    classifyDeclarator(parent, ctx);
  } else {
    ctx.findings.push(enumeratedFinding(path.node, ctx));
  }
}

/** Collect every env finding in one source file. */
function findEnvReads(code: string, file: string): Finding[] {
  const ast = parseFileAst(code);
  if (!ast) return [];
  const ctx: Ctx = { file, code, findings: [], seen: new Set() };
  traverse(ast, {
    MemberExpression(path) {
      if (isEnvBase(path.node) && !isShadowed(path.node, path.scope)) {
        classifyUse(path, ctx);
      }
    }
  });
  return ctx.findings;
}

function byLocation(a: EnvLocation, b: EnvLocation): number {
  return a.file.localeCompare(b.file) || a.line - b.line || a.column - b.column;
}

/** Group raw findings into the sorted, deduplicated report. */
function buildReport(
  findings: Finding[],
  filesAnalyzed: number
): EnvReadsReport {
  const varMap = new Map<string, EnvLocation[]>();
  const dynamic: EnvSiteUse[] = [];
  const enumerated: EnvSiteUse[] = [];
  for (const f of findings) {
    if (f.kind === "var") {
      const list = varMap.get(f.name) ?? [];
      list.push(f.loc);
      varMap.set(f.name, list);
    } else if (f.kind === "dynamic") {
      dynamic.push({ loc: f.loc, snippet: f.snippet });
    } else {
      enumerated.push({ loc: f.loc, snippet: f.snippet });
    }
  }
  const byVar = [...varMap.entries()]
    .map(([name, locations]) => ({
      name,
      locations: locations.sort(byLocation)
    }))
    .sort((a, b) => a.name.localeCompare(b.name));
  dynamic.sort((a, b) => byLocation(a.loc, b.loc));
  enumerated.sort((a, b) => byLocation(a.loc, b.loc));
  return { byVar, dynamic, enumerated, filesAnalyzed };
}

/** Inventory every env read across a set of sources. */
export function analyzeEnvReads(
  inputs: Array<{ file: string; code: string }>
): EnvReadsReport {
  const findings: Finding[] = [];
  for (const { file, code } of inputs)
    findings.push(...findEnvReads(code, file));
  return buildReport(findings, inputs.length);
}
