// Immutable-literal extraction: the things a minifier never renames.
//   S:<text>  string value (string literal, template chunk, folded "a"+"b")
//   P:<name>  property name (object key, member access .x / ["x"], class member, ESM export name)
//   N:<num>   number, normalised by value (1e3 == 1000); bigint as N:<digits>n
//   R:/p/f    regex literal
// Long values (> 120 chars) are kept as a sha1 prefix (S#..., R#...) to bound size; they still match exactly.
// mode "source": the npm file as published. Skips what bundling removes (import/export/require specifiers of
// bundled files, "use strict") and adds what bundling adds (ESM export names, which Bun/esbuild print as
// __export keys).
import { createRequire } from "node:module";
import crypto from "node:crypto";
const req = createRequire("/work/exp095-library-fingerprints/lab/tools/package.json");
const acorn = req("acorn");
const builtins = new Set(createRequire(import.meta.url)("node:module").builtinModules.filter((b) => !b.startsWith("_")));

const LONG = 120;
const normRaw = (kind, v) => (v.length > LONG ? `${kind}#${crypto.createHash("sha1").update(v).digest("hex").slice(0, 16)}` : `${kind}:${v}`);
// builtin module names are spelled with or without "node:" by different bundlers: one spelling
const norm = (kind, v) => (kind === "S" && isBuiltin(v) ? normRaw(kind, "node:" + v.replace(/^node:/, "")) : normRaw(kind, v));
const isBuiltin = (s) => builtins.has(s.replace(/^node:/, "")) || s.startsWith("node:");

function constStr(n) {
  if (!n) return null;
  if (n.type === "Literal" && typeof n.value === "string") return n.value;
  if (n.type === "TemplateLiteral" && n.expressions.length === 0) return n.quasis[0].value.cooked ?? n.quasis[0].value.raw;
  if (n.type === "BinaryExpression" && n.operator === "+") {
    const a = constStr(n.left), b = constStr(n.right);
    if (a != null && b != null) return a + b;
  }
  return null;
}

const PARSE_OPTS = { ecmaVersion: "latest", allowHashBang: true, allowReturnOutsideFunction: true, allowAwaitOutsideFunction: true, allowImportExportEverywhere: true, allowReserved: true, allowSuperOutsideMethod: true };

export function parse(code) {
  try { return { ast: acorn.parse(code, { ...PARSE_OPTS, sourceType: "module" }), how: "module" }; } catch {}
  try { return { ast: acorn.parse(code, { ...PARSE_OPTS, sourceType: "script" }), how: "script" }; } catch {}
  return { ast: null, how: "tokens" };
}

function tokensFallback(code, out) { // out is a LitSet
  // Fragments that do not parse: token stream. A name after "." is a property; strings/numbers/regex/templates as usual.
  let prev = null;
  try {
    for (const t of acorn.tokenizer(code, { ...PARSE_OPTS, sourceType: "script" })) {
      const ty = t.type.label;
      if (ty === "string") out.add(norm("S", t.value));
      else if (ty === "template" && t.value) out.add(norm("S", t.value));
      else if (ty === "num") out.add(`N:${String(t.value)}`);
      else if (ty === "regexp") out.add(norm("R", `/${t.value.pattern}/${t.value.flags}`));
      else if (ty === "name" && prev === ".") out.add(norm("P", t.value));
      prev = ty;
    }
  } catch {}
}

class LitSet extends Set { add(v) { if (v === "N:0" || v === "N:1") return this; if (v === "S:undefined") super.add("S:u"); return super.add(v); } }
export function extract(code, mode = "min") {
  const out = new LitSet();
  const { ast, how } = parse(code);
  if (!ast) { tokensFallback(code, out); return { lits: out, how }; } // eslint-disable-line
  const src = mode === "source";
  const propKey = (k, computed) => {
    if (!computed) {
      if (k.type === "Identifier") { out.add(norm("P", k.name)); return true; }
      if (k.type === "Literal") { if (typeof k.value === "string") out.add(norm("P", k.value)); else if (typeof k.value === "number") out.add(`N:${String(k.value)}`); return true; }
      if (k.type === "PrivateIdentifier") return true; // minifiers rename #private
      return false;
    }
    const s = constStr(k);
    if (s != null) { out.add(norm("P", s)); return true; }
    return false;
  };
  const exportName = (n) => { if (!src || !n) return; if (n.type === "Identifier") out.add(norm("P", n.name)); else if (n.type === "Literal") out.add(norm("P", String(n.value))); };
  const declNames = (d) => {
    if (!d) return;
    if (d.type === "FunctionDeclaration" || d.type === "ClassDeclaration") { if (d.id) exportName(d.id); }
    else if (d.type === "VariableDeclaration") for (const x of d.declarations) patNames(x.id);
  };
  const patNames = (p) => {
    if (!p) return;
    if (p.type === "Identifier") exportName(p);
    else if (p.type === "ObjectPattern") for (const q of p.properties) patNames(q.type === "RestElement" ? q.argument : q.value);
    else if (p.type === "ArrayPattern") for (const q of p.elements) patNames(q && (q.type === "RestElement" ? q.argument : q));
    else if (p.type === "AssignmentPattern") patNames(p.left);
  };
  const stack = [ast];
  while (stack.length) {
    const n = stack.pop();
    if (!n || typeof n.type !== "string") continue;
    switch (n.type) {
      case "Literal":
        if (typeof n.value === "string") out.add(norm("S", n.value));
        else if (typeof n.value === "number") out.add(`N:${String(n.value)}`);
        else if (n.bigint) out.add(`N:${n.bigint}n`);
        else if (n.regex) out.add(norm("R", `/${n.regex.pattern}/${n.regex.flags}`));
        continue;
      case "TemplateLiteral":
        for (const q of n.quasis) { const v = q.value.cooked ?? q.value.raw; if (v) out.add(norm("S", v)); }
        for (const e of n.expressions) stack.push(e);
        continue;
      case "BinaryExpression":
        if (n.operator === "+") { const s = constStr(n); if (s != null) out.add(norm("S", s)); }
        break;
      case "ExpressionStatement":
        if (n.directive) continue;
        break;
      case "Property":
        if (propKey(n.key, n.computed)) { if (n.value !== n.key && !n.shorthand) stack.push(n.value); else if (n.shorthand) { /* {a} -> {a:x} after minify: key name survives */ } continue; }
        break;
      case "MethodDefinition": case "PropertyDefinition":
        if (propKey(n.key, n.computed)) { if (n.value) stack.push(n.value); continue; }
        break;
      case "MemberExpression":
        if (propKey(n.property, n.computed)) { stack.push(n.object); continue; }
        break;
      case "ImportDeclaration":
        if (src) {
          // imported names survive as member accesses when the target is a wrapped/CJS/external module
          for (const s of n.specifiers) if (s.type === "ImportSpecifier" && s.imported) exportName(s.imported);
          const spec = constStr(n.source); if (spec != null && isBuiltin(spec)) out.add(norm("S", spec));
          continue;
        }
        break;
      case "ExportAllDeclaration":
        if (src) { if (n.exported) exportName(n.exported); continue; }
        break;
      case "ExportNamedDeclaration":
        if (src) { for (const s of n.specifiers) exportName(s.exported); declNames(n.declaration); if (n.declaration) stack.push(n.declaration); continue; }
        break;
      case "ExportDefaultDeclaration":
        if (src) { out.add("P:default"); stack.push(n.declaration); continue; }
        break;
      case "ImportExpression":
        if (src && constStr(n.source) != null && !isBuiltin(constStr(n.source))) continue;
        break;
      case "CallExpression":
        if (src && n.callee.type === "Identifier" && n.callee.name === "require" && n.arguments.length === 1) {
          const s = constStr(n.arguments[0]);
          if (s != null && !isBuiltin(s)) continue; // bundled: becomes a call to the factory, the string is gone
        }
        break;
    }
    for (const k in n) {
      if (k === "type" || k === "start" || k === "end" || k === "loc" || k === "range") continue;
      const v = n[k];
      if (Array.isArray(v)) { for (const c of v) if (c && typeof c.type === "string") stack.push(c); }
      else if (v && typeof v.type === "string") stack.push(v);
    }
  }
  return { lits: out, how };
}

export function extractJson(text) {
  const out = new LitSet();
  let v; try { v = JSON.parse(text); } catch { return { lits: out, how: "badjson" }; }
  const st = [v];
  while (st.length) {
    const x = st.pop();
    if (typeof x === "string") out.add(norm("S", x));
    else if (typeof x === "number") out.add(`N:${String(x)}`);
    else if (Array.isArray(x)) st.push(...x);
    else if (x && typeof x === "object") for (const [k, y] of Object.entries(x)) { out.add(norm("P", k)); st.push(y); }
  }
  return { lits: out, how: "json" };
}
