// WP4.2 capture hook (preloaded with `tsx --import`). Records, as JSONL
// under $WP42_CAPTURE_DIR:
//
//   requests.jsonl  — every CachedLLMProvider dispatch: the FULL typed
//                     request (Sets as arrays in their actual order), the
//                     params and the TS key (the WP4.1 hook's rows), plus —
//                     for a module-level request — the builder inputs its
//                     userPrompt was built from (`moduleInputs`).
//   code-window.jsonl — every selectFunctionCode / capContextCode call:
//                     inputs and output.
//   context.jsonl   — every buildContext call: the babel-side VIEW the
//                     builder reads (callee node facts, scope-chain binding
//                     names, parent-scope bindings) and its output.
//   scopes.jsonl    — a scope's binding-name list (and, for the program
//                     scope, its free names), written once per distinct
//                     snapshot; context rows reference it by id (the Bun
//                     module wrapper's scope is ~25k names).
//
// The prompt builders for module-level requests read inputs the dump does
// not carry (declarations, assignment/usage context, prior names, the
// eligibility predicate); they are captured at build time keyed by the
// prompt string they produced and joined to the dispatch by userPrompt.
//
// Observation only: nothing a decision reads is changed.
import fs from "node:fs";
import path from "node:path";
import { register } from "node:module";
import { createHash } from "node:crypto";

const dir = process.env.WP42_CAPTURE_DIR;
const src = process.env.WP42_SRC;
if (!dir || !src) throw new Error("WP42_CAPTURE_DIR / WP42_SRC unset");
fs.mkdirSync(dir, { recursive: true });

register(new URL("./wp42-capture-loader.mjs", import.meta.url));

const files = {};
function append(name, row) {
  files[name] ??= fs.openSync(path.join(dir, name), "w");
  fs.writeSync(files[name], `${JSON.stringify(row)}\n`);
}
const setsAsArrays = (_k, v) =>
  v instanceof Set ? [...v] : v instanceof Map ? Object.fromEntries(v) : v;

const t = await import("@babel/types");
const { generate } = await import(`${src}/src/babel-utils.ts`);
const { isPending } = await import(`${src}/src/rename/lifecycle.ts`);
const cached = await import(`${src}/src/llm/cached-provider.ts`);
const prompts = await import(`${src}/src/llm/prompts.ts`);

// ---- module-level prompt inputs, keyed by the prompt they produced ----
const moduleInputsByPrompt = new Map();
let lastModulePrompt = null;
function eligibilityOf(usedNames, isEligible) {
  return [...usedNames].map((n) => isEligible(n) === true);
}
function moduleArgs(args) {
  const [
    declarations,
    assignmentContext,
    usageExamples,
    identifiers,
    usedNames,
    isEligible,
    suggestedNames
  ] = args;
  return {
    declarations,
    assignmentContext,
    usageExamples,
    identifiers,
    usedNames: [...usedNames],
    usedEligible: eligibilityOf(usedNames, isEligible),
    suggestedNames
  };
}

let codeWindowSeq = 0;
let contextSeq = 0;
let moduleSeq = 0;

// ---- buildContext's babel view ----
// A scope's binding-name list is written ONCE per distinct snapshot to
// scopes.jsonl and referenced by id: the Bun module wrapper's function
// scope holds ~25k names and every buildContext call walks through it.
const SNAPSHOT_MIN = 64;
const scopeSnapshots = new WeakMap(); // Scope -> { id, key }
let scopeSnapshotSeq = 0;
function snapshotRef(scope, names, globals) {
  const key = `${names.join(",")}\u0000${globals.join(",")}`;
  const last = scopeSnapshots.get(scope);
  if (last && last.key === key) return last.id;
  const id = scopeSnapshotSeq++;
  scopeSnapshots.set(scope, { id, key });
  append("scopes.jsonl", { id, names, globals });
  return id;
}
function scopeEntry(scope) {
  const names = Object.keys(scope.bindings);
  return names.length < SNAPSHOT_MIN
    ? names
    : { ref: snapshotRef(scope, names, []) };
}
function programScopeRef(scope) {
  return snapshotRef(
    scope,
    Object.keys(scope.bindings),
    Object.keys(scope.globals || {})
  );
}
function genOr(node, fallback, opts) {
  try {
    return generate(node, opts).code;
  } catch {
    return fallback;
  }
}
function paramView(param) {
  if (t.isIdentifier(param)) return { kind: "identifier", name: param.name };
  if (t.isRestElement(param) && t.isIdentifier(param.argument))
    return { kind: "rest", name: param.argument.name };
  if (t.isAssignmentPattern(param) && t.isIdentifier(param.left))
    return { kind: "assign", name: param.left.name };
  return {
    kind: "other",
    code: genOr(param, "[code generation failed]", {
      compact: false,
      comments: false
    })
  };
}
function calleeView(callee) {
  const node = callee.path.node;
  const parent = callee.path.parent;
  return {
    nodeType: node.type,
    id: node.id ? node.id.name : null,
    declaratorId:
      t.isVariableDeclarator(parent) && t.isIdentifier(parent.id)
        ? parent.id.name
        : null,
    params: node.params.map(paramView),
    bodyCode: genOr(node.body, "[code generation failed]", {
      compact: false,
      comments: false
    })
  };
}
function bindingView(name, binding, isEligible) {
  const eligible = isEligible(name) === true;
  if (!eligible) return { name, eligible };
  const p = binding.path;
  let kind = "other";
  let code = "";
  if (p.isFunctionDeclaration() || p.isClassDeclaration()) kind = "fnOrClass";
  else if (p.isVariableDeclarator()) {
    kind = "declarator";
    code = p.parentPath ? genOr(p.parentPath.node, "") : "";
  } else code = genOr(p.node, "");
  return { name, eligible, kind, code };
}
function contextView(fn, isEligible) {
  const chain = [];
  let scope = fn.path.scope;
  let programRef = null;
  while (scope) {
    if (!scope.parent) programRef = programScopeRef(scope);
    else chain.push(scopeEntry(scope));
    scope = scope.parent;
  }
  const programParent = fn.path.scope.getProgramParent();
  const parentPending = !!(fn.scopeParent && isPending(fn.scopeParent));
  let parentBindings = null;
  if (parentPending) {
    parentBindings = Object.entries(fn.scopeParent.path.scope.bindings).map(
      ([name, b]) => bindingView(name, b, isEligible)
    );
  }
  return {
    callees: [...fn.internalCallees].map(calleeView),
    scopeChain: chain,
    programScope: programRef,
    programIsTop: programParent === scopeTop(fn.path.scope),
    parentPending,
    parentBindings
  };
}
function scopeTop(scope) {
  while (scope.parent) scope = scope.parent;
  return scope;
}
const sha = (s) => createHash("sha256").update(s).digest("hex");

globalThis.__wp42rec = (name, args, out) => {
  switch (name) {
    case "selectFunctionCode": {
      const sel = args[0];
      append("code-window.jsonl", {
        seq: codeWindowSeq++,
        fn: name,
        sel: {
          code: sel.code,
          sessionId: sel.sessionId,
          fnStartLine: sel.fnStartLine ?? null,
          fnEndLine: sel.fnEndLine ?? null,
          anchorStartLines: sel.anchorStartLines
            ? sel.anchorStartLines.map((l) => l ?? null)
            : null,
          identifierNames: sel.identifierNames ?? null
        },
        out
      });
      return;
    }
    case "capContextCode":
      append("code-window.jsonl", {
        seq: codeWindowSeq++,
        fn: name,
        code: args[0],
        sessionId: args[1],
        out
      });
      return;
    case "buildContext": {
      const [fn, , isEligible] = args;
      const used = [...out.usedIdentifiers];
      append("context.jsonl", {
        seq: contextSeq++,
        sessionId: fn.sessionId,
        view: contextView(fn, isEligible),
        out: {
          calleeSignatures: out.calleeSignatures,
          callsites: out.callsites,
          usedIdentifiersCount: used.length,
          usedIdentifiersSha: sha(used.join("\n")),
          contextVars: out.contextVars ?? null
        }
      });
      return;
    }
    case "buildModuleLevelRenamePrompt": {
      const inputs = moduleArgs(args);
      lastModulePrompt = { inputs, out };
      moduleInputsByPrompt.set(out, { prompt: inputs });
      append("module-builders.jsonl", {
        seq: moduleSeq++,
        fn: name,
        inputs,
        out
      });
      return;
    }
    case "buildModuleLevelRenameBody":
      append("module-builders.jsonl", {
        seq: moduleSeq++,
        fn: name,
        inputs: moduleArgs(args),
        out
      });
      return;
    case "buildModuleLevelRetryPrefix": {
      const [previousAttempt, failures] = args;
      append("module-builders.jsonl", {
        seq: moduleSeq++,
        fn: name,
        inputs: { previousAttempt, failures },
        out
      });
      if (lastModulePrompt) {
        moduleInputsByPrompt.set(`${out}\n${lastModulePrompt.out}`, {
          prompt: lastModulePrompt.inputs,
          retryPrefix: { previousAttempt, failures }
        });
      }
      return;
    }
  }
};

// ---- the dispatch rows (WP4.1's shape + moduleInputs) ----
const proto = cached.CachedLLMProvider.prototype;
const original = proto.suggestAllNames;
let seq = 0;
proto.suggestAllNames = function (request) {
  const key = cached.cacheKeyOf(request, this.params);
  const row = { seq: seq++, params: this.params, request, cacheKey: key };
  if (request.systemPrompt === prompts.MODULE_LEVEL_RENAME_SYSTEM_PROMPT) {
    row.moduleInputs = moduleInputsByPrompt.get(request.userPrompt) ?? null;
  }
  append("requests.jsonl", JSON.parse(JSON.stringify(row, setsAsArrays)));
  return original.call(this, request);
};
