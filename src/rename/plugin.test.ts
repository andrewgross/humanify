import assert from "node:assert";
import { createIsEligible } from "./rename-eligibility.js";
import { describe, it } from "node:test";
import { parseSync } from "@babel/core";
import { generate } from "../babel-utils.js";
import type { BatchRenameRequest, LLMProvider } from "../llm/types.js";
import {
  createRenamePlugin,
  getModuleLevelBindings,
  resolveFinalOutput
} from "./plugin.js";
import { applyRenameLedger } from "./rename-ledger.js";

const mockProvider: LLMProvider = {
  async suggestAllNames(request: BatchRenameRequest) {
    const renames: Record<string, string> = {};
    for (const id of request.identifiers) {
      renames[id] = `${id}Renamed`;
    }
    return { renames };
  }
};

describe("createRenamePlugin minted-token census (exp021 WS0)", () => {
  it("counts a class-expression inner id that escapes every naming path", async () => {
    // The outer binding `BaseError` gets named, but the class expression's
    // own id `uq` binds in the expression's inner scope that no collector
    // visits — the escape mechanism the naming floor targets. The census
    // must report it so `not renamed` is truthful.
    const rename = createRenamePlugin({ provider: mockProvider });
    const result = await rename(
      "var BaseError = class uq extends Error {};\nexport { BaseError };"
    );
    assert.strictEqual(result.parseFailure, undefined);
    const census = result.coverageData?.mintedCensus;
    assert.ok(census, "coverage must carry a minted census");
    assert.ok(
      census.byFamily.classExprId >= 1,
      `expected the class-expr id to be counted, got ${JSON.stringify(census.byFamily)}`
    );
  });

  it("reports zero minted leftovers when everything is named", async () => {
    const rename = createRenamePlugin({ provider: mockProvider });
    const result = await rename("function a() { var b = 1; return b; }");
    assert.strictEqual(result.coverageData?.mintedCensus?.total, 0);
  });

  it("--naming-floor derives the class-expr id and closes the census gap", async () => {
    const src =
      "var BaseError = class uq extends Error {};\nexport { BaseError };";
    const off = await createRenamePlugin({ provider: mockProvider })(src);
    assert.ok((off.coverageData?.mintedCensus?.byFamily.classExprId ?? 0) >= 1);
    assert.strictEqual(off.namingFloor, undefined);

    const on = await createRenamePlugin({
      provider: mockProvider,
      namingFloor: true
    })(src);
    assert.strictEqual(on.parseFailure, undefined);
    assert.strictEqual(on.semanticFailure, undefined);
    assert.strictEqual(on.namingFloor?.derived, 1);
    assert.strictEqual(
      on.coverageData?.mintedCensus?.byFamily.classExprId,
      0,
      "the class-expr id must be named after the floor"
    );
    // The derivation copies the outer binding's FINAL name (proving it runs
    // after the naming passes, not before).
    assert.match(on.code, /class BaseErrorRenamed extends Error/);
    assert.doesNotMatch(on.code, /class uq/);
  });

  it("--naming-floor-sweep wires the coverage sweep and stays a valid no-op when nothing escapes", async () => {
    // In a fresh run the LLM path names every param/var, so the sweep finds
    // no targets (the genuine param/var escape is a transfer-settle property
    // of the real lineage leg — see coverage-sweep.test.ts for the sweep
    // logic, and the exp021 offline harness for scale). Here we pin that the
    // sweep is invoked, reports its field, and never corrupts the output.
    const src =
      "function attachListener(callback) { return callback.call(null); }";
    const on = await createRenamePlugin({
      provider: mockProvider,
      namingFloor: true,
      namingFloorSweep: true
    })(src);
    assert.strictEqual(on.parseFailure, undefined);
    assert.strictEqual(on.semanticFailure, undefined);
    assert.strictEqual(typeof on.namingFloor?.swept, "number");
    // Nothing minified survives; the sweep is a clean no-op here.
    assert.strictEqual(on.coverageData?.mintedCensus?.total, 0);
  });

  it("--naming-floor without the sweep flag runs the deterministic passes only", async () => {
    // The class-expr id is derived deterministically (no LLM); the sweep
    // stays off without its flag, so swept is always 0.
    const src = "var BaseError = class uq extends Error {};\nlog(BaseError);";
    const on = await createRenamePlugin({
      provider: mockProvider,
      namingFloor: true
    })(src);
    assert.strictEqual(on.namingFloor?.derived, 1);
    assert.strictEqual(on.namingFloor?.swept, 0);
  });
});

describe("prior-aware naming-floor sweep (exp022)", () => {
  // The seam: a minted sweep target whose PRIOR counterpart is descriptive
  // must get the prior name via the reconcile pass's asymmetric tier — a
  // deterministic, cross-version-stable transfer — NOT a fresh LLM name
  // (which differs per leg and creates diff noise the descriptive tier
  // then rightly refuses to snap). Only targets with no usable prior
  // counterpart may go to the LLM.
  //
  // Fixture anatomy (each piece defeats a specific mechanism so the sweep
  // target survives to the seam):
  //   - `uq`/`rf` are class-EXPRESSION inner ids: they escape every
  //     collector, so the main LLM pass never names them.
  //   - each class self-references its outer binding in a static method,
  //     so the deterministic floor's derivation skips (capture-in-subtree)
  //     and the sweep is the only path left.
  //   - `uq`'s declaration line also carries `w9`→`BaseTask`, whose own
  //     declaration genuinely changed (makeHandler(1) vs (2)) and thus
  //     never reconciles: a fresh LLM name for `uq` becomes an
  //     unreconcilable descriptive↔descriptive pair (decl-not-clean),
  //     while a still-minted `uq` is the asymmetric tier's easy case.
  //   - `rf`'s whole class is NEW code (no prior counterpart): the sweep
  //     must still LLM-name it.
  const canon = (src: string): string => {
    const parsed = parseSync(src, {
      sourceType: "unambiguous",
      configFile: false,
      babelrc: false
    });
    assert.ok(parsed);
    return generate(parsed, { compact: false }).code;
  };

  it("transfers the prior name for an addressable sweep target and LLMs only the residue", async () => {
    // The bare console.log anchors carry no bindings, so they stay
    // byte-identical across legs and keep each changed line in its own
    // diff hunk (a merged hunk would let the genuinely-changed w9
    // declaration poison the class line's clean pair).
    const newSource = canon(`
      var w9 = makeHandler(1);
      console.log("anchor-one");
      var Q4 = class uq extends w9 {
        static of() {
          return new Q4();
        }
      };
      var K7 = class rf {
        static make() {
          return new K7();
        }
      };
      console.log(Q4, w9, K7);
    `);
    const priorCode = canon(`
      var BaseTask = makeHandler(2);
      console.log("anchor-one");
      var Q4 = class TaskRegistry extends BaseTask {
        static of() {
          return new Q4();
        }
      };
      console.log(Q4, BaseTask);
    `);

    const calls: BatchRenameRequest[] = [];
    const provider: LLMProvider = {
      async suggestAllNames(request: BatchRenameRequest) {
        calls.push(request);
        const renames: Record<string, string> = {};
        for (const id of request.identifiers) {
          // Poison name: applying it means the sweep LLM-named a target
          // whose prior counterpart (TaskRegistry) was transferable.
          if (id === "uq") renames[id] = "RegistryBase";
          else if (id === "rf") renames[id] = "registryFactory";
          // Prefix (not suffix): `w9Named` would still match the census's
          // Bun-token shape (1–2 letter head + digit) and fake a leftover.
          else renames[id] = `renamed${id}`;
        }
        return { renames };
      }
    };

    const result = await createRenamePlugin({
      provider,
      priorVersionCode: priorCode,
      reconcilePriorDiff: true,
      namingFloor: true,
      namingFloorSweep: true,
      emitRenameLedger: true
    })(newSource);

    assert.strictEqual(result.parseFailure, undefined);
    assert.strictEqual(result.semanticFailure, undefined);
    assert.match(
      result.code,
      /class TaskRegistry\b/,
      `the addressable target must carry the PRIOR name, got:\n${result.code}`
    );
    assert.doesNotMatch(
      result.code,
      /RegistryBase/,
      "a fresh LLM name must never win over a transferable prior name"
    );
    assert.ok(
      !calls.some((request) => request.identifiers.includes("uq")),
      "the sweep must not ask the LLM about a target with a prior counterpart"
    );
    assert.match(
      result.code,
      /registryFactory/,
      `the genuinely-new target must still be LLM-named, got:\n${result.code}`
    );
    assert.ok(
      (result.priorDiffReconciled?.renames ?? 0) >= 1,
      "the transfer must go through the reconcile pass"
    );
    assert.ok(
      (result.namingFloor?.swept ?? 0) >= 1,
      "the deferred sweep must report its applied names in floor stats"
    );
    assert.strictEqual(
      result.coverageData?.mintedCensus?.total,
      0,
      "census must reflect the final output: every minted binding resolved"
    );

    // End-to-end ledger proof: reconcile (uq→prior TaskRegistry) and the
    // deferred sweep (rf→registryFactory) both rename the post-generate code,
    // captured as chained `post` stages. Replaying the whole ledger reproduces
    // the FINAL shipped output — not just the LLM-rename output.
    assert.ok(result.renameLedger, "a ledger should be emitted");
    assert.ok(
      (result.renameLedger.ledger.post?.length ?? 0) >= 2,
      "reconcile + deferred-sweep passes must each be a post stage"
    );
    assert.strictEqual(
      applyRenameLedger(result.renameLedger.source, result.renameLedger.ledger),
      result.code,
      "ledger replay must reproduce the final output (reconcile + sweep incl.)"
    );
  });
});

describe("createRenamePlugin sourceMap", () => {
  it("sourceMap is null when not requested", async () => {
    const rename = createRenamePlugin({ provider: mockProvider });
    const result = await rename("function a() { return 1; }");

    assert.strictEqual(result.sourceMap, null);
  });

  it("sourceMap is produced when sourceMap: true", async () => {
    const rename = createRenamePlugin({
      provider: mockProvider,
      sourceMap: true
    });
    const result = await rename("function a() { return 1; }");

    assert.ok(result.sourceMap, "sourceMap should not be null");
    assert.strictEqual(result.sourceMap.version, 3);
    assert.ok(result.sourceMap.mappings, "mappings should be non-empty");
    assert.ok(
      Array.isArray(result.sourceMap.sources),
      "sources should be an array"
    );
  });

  it("sourceMap sources uses sourceFileName", async () => {
    const rename = createRenamePlugin({
      provider: mockProvider,
      sourceMap: true
    });
    const result = await rename("function a() { return 1; }");

    assert.ok(result.sourceMap);
    assert.ok(result.sourceMap.sources.includes("input.js"));
  });

  it("sourceMap is null for empty function list with sourceMap: false", async () => {
    const rename = createRenamePlugin({ provider: mockProvider });
    // Code with no functions
    const result = await rename("var x = 1;");

    assert.strictEqual(result.sourceMap, null);
  });

  it("sourceMap is produced for empty function list with sourceMap: true", async () => {
    const rename = createRenamePlugin({
      provider: mockProvider,
      sourceMap: true
    });
    // Code with no functions — hits the early return path
    const result = await rename("var x = 1;");

    assert.ok(
      result.sourceMap,
      "sourceMap should be produced even with no functions"
    );
    assert.strictEqual(result.sourceMap.version, 3);
  });
});

describe("prior-version function declaration transfer", () => {
  it("transfers function declaration name from prior version", async () => {
    // Minified code with a function declaration
    const currentCode = `function a(e, t) { return Object.assign({}, e, t); }`;
    // Prior humanified version — same structure, renamed
    const priorCode = `function mergeObjects(e, t) { return Object.assign({}, e, t); }`;

    const rename = createRenamePlugin({
      provider: mockProvider,
      priorVersionCode: priorCode
    });

    const result = await rename(currentCode);

    // The function declaration name should be transferred
    assert.ok(
      result.code.includes("mergeObjects"),
      `function name should be transferred from prior version, got:\n${result.code}`
    );
    assert.ok(
      !result.code.includes("function a("),
      `original minified name should be replaced, got:\n${result.code}`
    );
  });

  it("transfers close-match function declaration name", async () => {
    // Current code with function declaration
    const currentCode = `function a(e, t) { return Object.assign({}, e, t); }`;
    // Prior version — slightly different structure (extra statement) so it's a close match
    const priorCode = `function mergeObjects(e, t) { var r = Object.assign({}, e, t); return r; }`;

    const rename = createRenamePlugin({
      provider: mockProvider,
      priorVersionCode: priorCode
    });

    const result = await rename(currentCode);

    // For close-match, the function name should be transferred directly
    // (remaining identifiers go through LLM with prior context)
    assert.ok(
      result.code.includes("mergeObjects"),
      `function name should be transferred via close-match, got:\n${result.code}`
    );
  });

  it("transfers body-local binding in nested block scope (for-in)", async () => {
    // Function with a for-in loop declaring a block-scoped variable
    const currentCode = `function a(e, t) { for (var n in e) { t[n] = e[n]; } return t; }`;
    // Prior version: same structure, body-local n renamed to nextState
    const priorCode = `function mergeObjects(e, t) { for (var nextState in e) { t[nextState] = e[nextState]; } return t; }`;

    const rename = createRenamePlugin({
      provider: mockProvider,
      priorVersionCode: priorCode
    });

    const result = await rename(currentCode);

    // The body-local for-in binding should be transferred
    assert.ok(
      result.code.includes("nextState"),
      `body-local for-in binding should be transferred, got:\n${result.code}`
    );
    assert.ok(
      result.code.includes("mergeObjects"),
      `function name should be transferred, got:\n${result.code}`
    );
  });

  it("transfers module-level bindings via module binding matching", async () => {
    // Function references a module-level binding (assign) that it doesn't own
    const currentCode = `
      var a = Object.assign;
      function b(e, t) { return a({}, e, t); }
    `;
    // Prior version: assign renamed, function renamed
    const priorCode = `
      var mergeAssign = Object.assign;
      function mergeObjects(e, t) { return mergeAssign({}, e, t); }
    `;

    const rename = createRenamePlugin({
      provider: mockProvider,
      priorVersionCode: priorCode
    });

    const result = await rename(currentCode);

    // Function name should be transferred via function matching
    assert.ok(
      result.code.includes("mergeObjects"),
      `function name should be transferred, got:\n${result.code}`
    );
    // Module-level binding should be transferred via module binding matching
    // (not via function transfer — it's an external reference)
    assert.ok(
      result.code.includes("mergeAssign"),
      `module-level binding should be transferred via module binding matching, got:\n${result.code}`
    );
    assert.strictEqual(
      result.priorVersionBindingsApplied,
      1,
      "should report 1 module binding matched"
    );
  });
});

describe("transfer validation", () => {
  it("does not create duplicate declarations when two matches transfer the same name into one scope", async () => {
    // Prior version: two structurally distinct arrows, each var-named `go`
    // in its own (legal) scope.
    const priorCode = `
      function withA() {
        let go = () => fetchX(1);
        return go;
      }
      function withB() {
        let go = (x) => fetchY(x, 2);
        return go;
      }
    `;
    // Current version: both arrows now live in the SAME scope. Blindly
    // transferring both var names produces a duplicate \`let go\`.
    const currentCode = `
      function c() {
        let p = () => q(1);
        let r = (x) => s(x, 2);
        return [p, r];
      }
    `;

    const rename = createRenamePlugin({
      provider: mockProvider,
      priorVersionCode: priorCode
    });

    const result = await rename(currentCode);

    assert.strictEqual(
      result.parseFailure,
      undefined,
      `output must parse, got failure: ${result.parseFailure?.message}\n${result.parseFailure?.excerpt ?? ""}`
    );
    const goDeclarations = result.code.match(/let go\b/g) ?? [];
    assert.strictEqual(
      goDeclarations.length,
      1,
      `exactly one binding should receive the transferred name, got:\n${result.code}`
    );
  });
});

describe("internal error surfacing", () => {
  it("reports zero internalErrors on a clean run", async () => {
    const result = await createRenamePlugin({ provider: mockProvider })(
      `function a(x) { return x + 1; }`
    );
    assert.strictEqual(result.internalErrors, 0);
  });
});

describe("sibling-block duplicate binding names", () => {
  it("renames both same-named bindings in sibling blocks", async () => {
    // Two `let e` bindings in SIBLING blocks are different bindings with
    // one name. Name-keyed collection dropped one, leaving a minified
    // name behind — cross-version noise every run after.
    const code = `
      function pick(flag) {
        if (flag) {
          let e = flag + 1;
          for (let i = 0; i < 3; i++) { if (e > i) console.log(i); }
          return e;
        } else {
          let e = flag - 1;
          console.log("negative branch", e);
          return e;
        }
      }
      console.log(pick(2));
    `;

    const suffixing: LLMProvider = {
      async suggestAllNames(request: BatchRenameRequest) {
        const renames: Record<string, string> = {};
        for (const id of request.identifiers) {
          renames[id] = `${id}Named`;
        }
        return { renames };
      }
    };

    const rename = createRenamePlugin({ provider: suffixing });
    const result = await rename(code);

    assert.strictEqual(result.parseFailure, undefined);
    assert.ok(
      !/\blet e\b/.test(result.code),
      `both sibling-block bindings must be renamed, got:\n${result.code}`
    );
  });

  it("renames ALL of many same-named sibling bindings (the reroll families)", async () => {
    // Bun reuses one tiny name across MANY sibling block scopes of a big
    // function (`$_`×34, `v6`×36 in the Claude Code fixtures). Name-keyed
    // collection reaches at most two of them (main pass + shadowed pass);
    // the rest stay minified in BOTH legs of a cross-version run and Bun
    // rerolls the token between builds — the `$_→w_`/`v6→X6` noise
    // families. Every one of them must end up renamed.
    const code = `
      function dispatch(input) {
        if (input.a) {
          let K = readA(input);
          useA(K, input);
        }
        if (input.b) {
          let K = readB(input);
          useB(K, input);
        }
        if (input.c) {
          let K = readC(input);
          useC(K, input);
        }
        try {
          runAll(input);
        } catch (K) {
          reportFailure(K);
        }
        return input;
      }
      console.log(dispatch);
    `;

    const suffixing: LLMProvider = {
      async suggestAllNames(request: BatchRenameRequest) {
        const renames: Record<string, string> = {};
        for (const id of request.identifiers) {
          renames[id] = `${id}Named`;
        }
        return { renames };
      }
    };

    const rename = createRenamePlugin({ provider: suffixing });
    const result = await rename(code);

    assert.strictEqual(result.parseFailure, undefined);
    assert.strictEqual(result.semanticFailure, undefined);
    assert.ok(
      !/\blet K\b/.test(result.code) && !/catch \(K\)/.test(result.code),
      `all four same-named bindings must be renamed, got:\n${result.code}`
    );
  });
});

describe("eval/with soundness guard", () => {
  it("freezes bindings visible at a with site and keeps renaming the rest", async () => {
    // `with (obj)` resolves bare identifiers against obj at runtime —
    // renaming anything visible at the site (risky's and outer's own
    // bindings, module bindings) can silently change behavior. `safe` is
    // off the scope chain and must still be renamed.
    const code = `
      var moduleFlag = 1;
      function outer(cfg) {
        function safe(x) {
          for (let i = 0; i < 3; i++) { if (x > i) console.log(i); }
          return x * 2;
        }
        function risky(obj) {
          with (obj) { doThing(moduleFlag); }
        }
        risky(cfg);
        return safe(cfg.n);
      }
      outer({ n: 1 });
    `;

    const suffixing: LLMProvider = {
      async suggestAllNames(request: BatchRenameRequest) {
        const renames: Record<string, string> = {};
        for (const id of request.identifiers) {
          renames[id] = `${id}Named`;
        }
        return { renames };
      }
    };

    const rename = createRenamePlugin({ provider: suffixing });
    const result = await rename(code);

    assert.strictEqual(result.parseFailure, undefined);
    assert.match(
      result.code,
      /function risky\(obj\)/,
      `tainted function's bindings must keep their names, got:\n${result.code}`
    );
    assert.match(
      result.code,
      /function outer\(cfg\)/,
      "enclosing function on the scope chain is frozen too"
    );
    assert.match(
      result.code,
      /var moduleFlag = 1/,
      "module bindings are visible at the site and must not be renamed"
    );
    assert.match(
      result.code,
      /xNamed/,
      `functions off the scope chain still rename, got:\n${result.code}`
    );
  });
});

describe("nested function declaration ownership", () => {
  it("names each function declaration exactly once — the child owns its name", async () => {
    // `inner`'s name binding lives in outer's scope, so both batches used
    // to include it: inner named itself first (leaf-first), then outer's
    // batch renamed it AGAIN, discarding the child's self-chosen name.
    // With a suffixing mock the double pass is visible as a double suffix.
    const code = `
      function outer(seed) {
        function inner(x) {
          for (let i = 0; i < 3; i++) { if (x > i) console.log(i); }
          return x + seed;
        }
        return inner(seed);
      }
      console.log(outer(1));
    `;

    const suffixing: LLMProvider = {
      async suggestAllNames(request: BatchRenameRequest) {
        const renames: Record<string, string> = {};
        for (const id of request.identifiers) {
          renames[id] = `${id}Named`;
        }
        return { renames };
      }
    };

    const rename = createRenamePlugin({ provider: suffixing });
    const result = await rename(code);

    assert.strictEqual(result.parseFailure, undefined);
    assert.ok(
      !/NamedNamed/.test(result.code),
      `no identifier may be renamed twice, got:\n${result.code}`
    );
  });
});

describe("propagated external references", () => {
  it("does not propagate a module binding name from a single external ref", async () => {
    // Module binding has different init (so structural hash differs, no
    // direct match) and exactly ONE matched function references it. One
    // vote is one function's testimony — below the ≥2-vote floor, the
    // binding keeps its name for the LLM. The function name still
    // transfers.
    const currentCode = `
      var a = [1, 2, 3];
      function b(e) { return a.includes(e); }
    `;
    const priorCode = `
      var allowedValues = [1, 2, 3, 4];
      function isAllowed(e) { return allowedValues.includes(e); }
    `;

    const rename = createRenamePlugin({
      provider: mockProvider,
      priorVersionCode: priorCode
    });

    const result = await rename(currentCode);

    assert.ok(
      result.code.includes("isAllowed"),
      `function name should be transferred, got:\n${result.code}`
    );
    assert.ok(
      !result.code.includes("allowedValues"),
      `a single vote must not rename the module binding, got:\n${result.code}`
    );
  });

  it("propagates module binding with multiple agreeing votes", async () => {
    // Two functions both reference the same module binding
    const currentCode = `
      var a = [1, 2, 3];
      function b(e) { return a.includes(e); }
      function c(e) { a.push(e); return a; }
    `;
    const priorCode = `
      var allowedValues = [1, 2, 3, 4];
      function isAllowed(e) { return allowedValues.includes(e); }
      function addAllowed(e) { allowedValues.push(e); return allowedValues; }
    `;

    const rename = createRenamePlugin({
      provider: mockProvider,
      priorVersionCode: priorCode
    });

    const result = await rename(currentCode);

    assert.ok(
      result.code.includes("allowedValues"),
      `module binding should be propagated with multiple votes, got:\n${result.code}`
    );
  });

  it("does not double-rename module bindings already matched by structural hash", async () => {
    // Both binding AND function match — binding matched by structural hash,
    // external ref is redundant and should not cause issues
    const currentCode = `
      var a = Object.assign;
      function b(e, t) { return a({}, e, t); }
    `;
    const priorCode = `
      var mergeAssign = Object.assign;
      function mergeObjects(e, t) { return mergeAssign({}, e, t); }
    `;

    const rename = createRenamePlugin({
      provider: mockProvider,
      priorVersionCode: priorCode
    });

    const result = await rename(currentCode);

    // Should work correctly — binding matched by structural hash,
    // propagation finds no unmatched binding with the name
    assert.ok(
      result.code.includes("mergeAssign"),
      `module binding should be renamed (by structural hash), got:\n${result.code}`
    );
    assert.ok(
      result.code.includes("mergeObjects"),
      `function name should be transferred, got:\n${result.code}`
    );
  });
});

describe("close-match set elimination suggestedName", () => {
  // For close match to trigger, functions must have different structural hashes
  // but similar feature vectors. An extra if-statement changes the hash.

  it("sets suggestedName on module binding when 1:1 elimination succeeds", async () => {
    // Close match: extra if-statement in prior version changes AST structure.
    // Both reference one external: a (prior: counter).
    // After var name transfer, counter is the only unresolved prior external
    // and a is the only unresolved new external → 1:1 → suggestedName.
    const currentCode = `
      var a = 0;
      var b = function(x) { a++; return a + x; };
    `;
    const priorCode = `
      var counter = 0;
      var increment = function(x) { if (x > 0) counter++; return counter + x; };
    `;

    const suggestingProvider: LLMProvider = {
      async suggestAllNames(request: BatchRenameRequest) {
        const result: Record<string, string> = {};
        for (const id of request.identifiers) {
          result[id] = `${id}Renamed`;
        }
        return { renames: result };
      }
    };

    const rename = createRenamePlugin({
      provider: suggestingProvider,
      priorVersionCode: priorCode
    });

    const result = await rename(currentCode);

    // The function var name should be transferred via close match
    assert.ok(
      result.code.includes("increment"),
      `function var name should be transferred via close match, got:\n${result.code}`
    );
  });

  it("does NOT set suggestedName when elimination leaves >1 candidates", async () => {
    // Two unresolved externals on each side — no 1:1 elimination possible.
    // Use named function declarations (not anonymous expressions) so placeholder
    // mapping doesn't accidentally transfer body locals via $0/$1 positions.
    // Extra if-statement ensures close match (not exact).
    const currentCode = `
      var a = [1, 2];
      var c = {x: 1};
      function b(x) { a.push(x); return c.x + a.length + x; }
    `;
    const priorCode = `
      var counter = [1, 2, 3];
      var greeting = {x: 1, y: 2};
      function increment(x) { if (x > 0) counter.push(x); return greeting.x + counter.length + x; }
    `;

    const suggestingProvider: LLMProvider = {
      async suggestAllNames(request: BatchRenameRequest) {
        const result: Record<string, string> = {};
        for (const id of request.identifiers) {
          result[id] = `${id}Renamed`;
        }
        return { renames: result };
      }
    };

    const rename = createRenamePlugin({
      provider: suggestingProvider,
      priorVersionCode: priorCode
    });

    const result = await rename(currentCode);

    // Neither binding should get the prior name via set elimination
    // (>1 candidates on each side prevents 1:1 match)
    assert.ok(
      !result.code.includes("counter"),
      `should not auto-suggest 'counter' when >1 candidates remain, got:\n${result.code}`
    );
    assert.ok(
      !result.code.includes("greeting"),
      `should not auto-suggest 'greeting' when >1 candidates remain, got:\n${result.code}`
    );
  });

  it("eliminates already-resolved pairs before counting", async () => {
    // Two externals: a (resolved by structural hash) and c (unresolved).
    // Extra if-guard ensures close match (not exact) for the function.
    // c has different init across versions so it won't match by structural hash.
    // After eliminating mergeAssign from resolved set, c→totalCount is 1:1.
    const currentCode = `
      var a = Object.assign;
      var c = [1, 2];
      var b = function(x) { return a({}, {val: c.length + x}); };
    `;
    const priorCode = `
      var mergeAssign = Object.assign;
      var totalCount = [1, 2, 3];
      var createMerged = function(x) { if (x > 0) return mergeAssign({}, {val: totalCount.length + x}); return null; };
    `;

    const suggestingProvider: LLMProvider = {
      async suggestAllNames(request: BatchRenameRequest) {
        const result: Record<string, string> = {};
        for (const id of request.identifiers) {
          result[id] = `${id}Renamed`;
        }
        return { renames: result };
      }
    };

    const rename = createRenamePlugin({
      provider: suggestingProvider,
      priorVersionCode: priorCode
    });

    const result = await rename(currentCode);

    // `a` should be resolved by structural hash → `mergeAssign`
    assert.ok(
      result.code.includes("mergeAssign"),
      `'a' should match 'mergeAssign' by structural hash, got:\n${result.code}`
    );
    // `b` close-matches `createMerged` — function var name should transfer
    assert.ok(
      result.code.includes("createMerged"),
      `function var name should transfer via close match, got:\n${result.code}`
    );
  });
});

describe("wrapper-scope class declarations", () => {
  it("names a module-scope class declaration (classes are not FunctionNodes)", async () => {
    // shouldSkipBinding excluded class declarations claiming they are
    // "processed as FunctionNodes" — classes never become graph nodes,
    // so they fell through BOTH naming paths in both legs of a
    // cross-version run: the y6→C6 / HK→qK / m3→a3 reroll families
    // (1,326 occurrences in the exp015 run-2 diff).
    const code = `
      class y6 {
        constructor() {
          this.items = [];
        }
        add(item) {
          this.items.push(item);
          return this.items.length;
        }
      }
      class w4 extends y6 {
        describe() {
          return "queue of " + this.items.length;
        }
      }
      console.log(new w4().add(1));
    `;

    const suffixing: LLMProvider = {
      async suggestAllNames(request: BatchRenameRequest) {
        const renames: Record<string, string> = {};
        for (const id of request.identifiers) {
          renames[id] = `${id}Named`;
        }
        return { renames };
      }
    };

    const rename = createRenamePlugin({ provider: suffixing });
    const result = await rename(code);

    assert.strictEqual(result.parseFailure, undefined);
    assert.strictEqual(result.semanticFailure, undefined);
    assert.ok(
      !/class y6\b/.test(result.code) && !/class w4\b/.test(result.code),
      `class declarations must be renamed, got:\n${result.code}`
    );
    assert.match(result.code, /extends y6Named/);
  });

  it("transfers a class name via agreeing votes from exact-matched functions", async () => {
    // The convergence path: two exact-matched functions reference the
    // class; their external-ref votes must land the prior name on the
    // class binding (two votes clear the module-binding floor).
    const priorCode = `
      class TaskQueue {
        constructor() {
          this.items = [];
        }
      }
      function makeQueue(size) {
        for (let i = 0; i < size; i++) { if (i > 2) trace(i); }
        return new TaskQueue();
      }
      function checkQueue(x) {
        for (let j = 0; j < 4; j++) { if (x.deep > j) probe(j); }
        return x instanceof TaskQueue;
      }
      console.log(makeQueue, checkQueue);
    `;
    const v2Code = `
      class q9 {
        constructor() {
          this.items = [];
        }
      }
      function mk(size) {
        for (let i = 0; i < size; i++) { if (i > 2) trace(i); }
        return new q9();
      }
      function chk(x) {
        for (let j = 0; j < 4; j++) { if (x.deep > j) probe(j); }
        return x instanceof q9;
      }
      console.log(mk, chk);
    `;

    const suffixing: LLMProvider = {
      async suggestAllNames(request: BatchRenameRequest) {
        const renames: Record<string, string> = {};
        for (const id of request.identifiers) {
          renames[id] = `${id}Fresh`;
        }
        return { renames };
      }
    };

    const rename = createRenamePlugin({
      provider: suffixing,
      priorVersionCode: priorCode
    });
    const result = await rename(v2Code);

    assert.strictEqual(result.parseFailure, undefined);
    assert.match(
      result.code,
      /class TaskQueue\b/,
      `two agreeing votes must transfer the class name, got:\n${result.code}`
    );
  });
});

describe("module binding declaration text", () => {
  it("caps a giant SINGLE-LINE declarator (base64 blob) by chars", () => {
    // `var MF5 = "<205KB base64>"` is ONE line — a line cap passes it
    // whole and the batch 400-fails at ~45K tokens in every run
    // (exp015: the only surviving context failure after the line cap).
    const blob = "A".repeat(200_000);
    const code = `var MF5 = "${blob}";\nconsole.log(MF5);`;

    const ast = parseSync(code, { sourceType: "unambiguous" });
    assert.ok(ast);
    const result = getModuleLevelBindings(ast, createIsEligible());
    assert.ok(result);

    const mf5 = result.bindings.find((b: { name: string }) => b.name === "MF5");
    assert.ok(mf5, "MF5 should be a module binding");
    assert.ok(
      mf5.declaration.length <= 1100,
      `declaration text must be char-capped, got ${mf5.declaration.length} chars`
    );
  });

  it("caps a giant declarator so the prompt profile stays bounded", () => {
    // A multi-thousand-line object-literal declaration used to be embedded
    // WHOLE in the module-binding prompt profile, overflowing the model
    // context and 400-failing the batch (exp015 baseline: the fresh-leg
    // module-binding-batch failures).
    const entries = Array.from(
      { length: 600 },
      (_, i) => `  key${i}: value${i}`
    ).join(",\n");
    const code = `var gq = {\n${entries}\n};\nconsole.log(gq);`;

    const ast = parseSync(code, { sourceType: "unambiguous" });
    assert.ok(ast);
    const result = getModuleLevelBindings(ast, createIsEligible());
    assert.ok(result);

    const gq = result.bindings.find((b: { name: string }) => b.name === "gq");
    assert.ok(gq, "gq should be a module binding");
    const declLines = gq.declaration.split("\n").length;
    assert.ok(
      declLines <= 11,
      `declaration text must be capped, got ${declLines} lines`
    );
  });
});

describe("shouldSkipBinding in wrapper mode", () => {
  it("skips function declarations inside wrapper IIFE from module bindings", () => {
    // Simulate a Bun-style CJS wrapper IIFE with enough bindings to trigger
    // wrapper detection (>50 bindings). Function declarations inside should
    // NOT appear as module bindings — they're handled as FunctionNodes.
    const varDecls = Array.from(
      { length: 55 },
      (_, i) => `var v${i} = ${i};`
    ).join("\n");
    const code = `(function(exports, require, module) {\n${varDecls}\nfunction a(x) { return x + 1; }\nfunction b(y) { return y * 2; }\n});`;

    const ast = parseSync(code, { sourceType: "unambiguous" });
    assert.ok(ast, "should parse code");
    const result = getModuleLevelBindings(ast, createIsEligible());

    assert.ok(result, "should detect module-level bindings");

    const bindingNames = result.bindings.map((b: { name: string }) => b.name);
    assert.ok(
      !bindingNames.includes("a"),
      `function declaration 'a' should NOT be a module binding, got: [${bindingNames.join(", ")}]`
    );
    assert.ok(
      !bindingNames.includes("b"),
      `function declaration 'b' should NOT be a module binding, got: [${bindingNames.join(", ")}]`
    );
    // var declarations should still be present
    assert.ok(
      bindingNames.includes("v0"),
      `var declaration 'v0' should be a module binding`
    );
  });
});

describe("shouldSkipBinding with Bun CJS classification", () => {
  it("skips bindings inside a CJS factory body when source is provided", () => {
    const code = [
      "var A = (q, _) => () => (_ || q((_ = {exports: {}}).exports, _), _.exports);",
      "var lib = A((q, _) => {",
      "  var innerVar = 1;",
      "  function innerFn() { return innerVar; }",
      "  _.exports = innerFn;",
      "});",
      "var appVar = lib();"
    ].join("\n");

    const ast = parseSync(code, { sourceType: "unambiguous" });
    assert.ok(ast);

    const result = getModuleLevelBindings(ast, createIsEligible(), code);
    assert.ok(result, "should detect module-level bindings");

    const bindingNames = result.bindings.map((b: { name: string }) => b.name);

    // Outer factory var and the app var should be kept.
    assert.ok(bindingNames.includes("lib"));
    assert.ok(bindingNames.includes("appVar"));

    // Inner bindings should be classified as third-party and skipped.
    assert.ok(
      !bindingNames.includes("innerVar"),
      `innerVar should be skipped, got: [${bindingNames.join(", ")}]`
    );
    assert.ok(
      !bindingNames.includes("innerFn"),
      `innerFn should be skipped, got: [${bindingNames.join(", ")}]`
    );

    assert.ok(result.classification);
    assert.strictEqual(result.classification.factories.length, 1);
  });
});

describe("resolveFinalOutput (post-pass AST lifecycle)", () => {
  it("re-parses the SHIPPING code when the reconcile AST was released pre-sweep", () => {
    // Non-ledger runs release recon.ast before the deferred sweep (the sweep
    // consumes recon.code, a string; holding the AST doubles the live set).
    // When the sweep then applies nothing, the fallback parse must target the
    // shipping code (recon.code), not the pre-reconcile output.
    const { finalCode, finalAst } = resolveFinalOutput(
      "var alpha = 1;",
      {
        stats: { renames: 1, skipped: 0 },
        renames: [{ fromName: "alpha", toName: "beta" }],
        code: "var beta = 1;",
        ast: undefined
      },
      undefined,
      undefined,
      null
    );
    assert.strictEqual(finalCode, "var beta = 1;");
    assert.strictEqual(generate(finalAst).code, "var beta = 1;");
  });
});
