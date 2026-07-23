import assert from "node:assert";
import { describe, it } from "node:test";
import { parseSync } from "@babel/core";
import type * as t from "@babel/types";
import { generate } from "../babel-utils.js";
import {
  collectWordTokens,
  computeNormalDiff,
  parseNormalDiff,
  type ReconcileOptions,
  reconcileDiffNoise,
  tokenizeLine
} from "./diff-reconcile.js";
import { strategyTrail } from "./strategy-trail.js";

/**
 * The reconciliation is a pure function of (new text, prior text). Both
 * legs in production are babel-generator outputs, so the tests normalize
 * hand-written fixtures through generate() first — apply-mode assertions
 * can then demand byte-equality with the prior text whenever every
 * difference between the legs is reconcilable rename noise.
 */
function canon(code: string): string {
  const ast = parseSync(code, {
    sourceType: "unambiguous",
    configFile: false,
    babelrc: false
  });
  assert.ok(ast, "fixture must parse");
  return generate(ast, { compact: false }).code;
}

function run(
  priorRaw: string,
  newRaw: string,
  opts: Partial<ReconcileOptions> = {}
) {
  const priorText = canon(priorRaw);
  const newText = canon(newRaw);
  const newAst = parseSync(newText, {
    sourceType: "unambiguous",
    configFile: false,
    babelrc: false
  }) as t.File;
  assert.ok(newAst, "new text must parse");
  const diffText = computeNormalDiff(priorText, newText);
  const result = reconcileDiffNoise(newAst, diffText, opts);
  const output = generate(newAst, { compact: false }).code;
  return { result, priorText, newText, output };
}

function skipReasons(result: { skipped: Array<{ reason: string }> }): string[] {
  return result.skipped.map((s) => s.reason);
}

describe("computeNormalDiff", () => {
  it("produces normal-format change hunks", () => {
    const diff = computeNormalDiff("a\nb\nc\n", "a\nX\nc\n");
    assert.match(diff, /^2c2\n< b\n---\n> X$/m);
  });

  it("returns empty string for identical inputs", () => {
    assert.strictEqual(computeNormalDiff("same\n", "same\n"), "");
  });
});

describe("parseNormalDiff", () => {
  it("parses single-line and multi-line change hunks with 1-based ranges", () => {
    const hunks = parseNormalDiff(
      [
        "3c3",
        "< old",
        "---",
        "> new",
        "10,11c12,13",
        "< a",
        "< b",
        "---",
        "> c",
        "> d"
      ].join("\n")
    );
    assert.strictEqual(hunks.length, 2);
    assert.deepStrictEqual(hunks[0], {
      op: "c",
      priorStart: 3,
      newStart: 3,
      priorLines: ["old"],
      newLines: ["new"]
    });
    assert.deepStrictEqual(hunks[1], {
      op: "c",
      priorStart: 10,
      newStart: 12,
      priorLines: ["a", "b"],
      newLines: ["c", "d"]
    });
  });

  it("keeps add/delete hunks with their op so changed-line coverage is complete", () => {
    const hunks = parseNormalDiff(
      ["5a6,7", "> added1", "> added2", "9d9", "< gone"].join("\n")
    );
    assert.strictEqual(hunks.length, 2);
    assert.strictEqual(hunks[0].op, "a");
    assert.strictEqual(hunks[0].newStart, 6);
    assert.deepStrictEqual(hunks[0].newLines, ["added1", "added2"]);
    assert.strictEqual(hunks[1].op, "d");
    assert.deepStrictEqual(hunks[1].priorLines, ["gone"]);
    assert.deepStrictEqual(hunks[1].newLines, []);
  });
});

describe("tokenizeLine", () => {
  it("emits identifier tokens with 0-based columns and keeps punctuation verbatim", () => {
    const tokens = tokenizeLine("foo = bar(1);");
    const idents = tokens?.filter((tok) => tok.kind === "ident");
    assert.deepStrictEqual(idents, [
      { kind: "ident", text: "foo", col: 0 },
      { kind: "ident", text: "bar", col: 6 }
    ]);
  });

  it("treats string literal contents as opaque text, not identifiers", () => {
    const tokens = tokenizeLine('f("claude", x);');
    const idents = tokens?.filter((tok) => tok.kind === "ident") ?? [];
    assert.deepStrictEqual(
      idents.map((tok) => tok.text),
      ["f", "x"]
    );
  });

  it("keeps reserved words verbatim so keyword changes read as genuine", () => {
    const tokens = tokenizeLine("return value;");
    assert.ok(tokens);
    const returnTok = tokens.find((tok) => tok.text === "return");
    assert.strictEqual(returnTok?.kind, "text");
  });

  it("tokenizes identifiers inside template expressions but not quasi text", () => {
    // biome-ignore lint/suspicious/noTemplateCurlyInString: the plain string IS the line under test
    const tokens = tokenizeLine("const p = `pre ${dirName} post`;");
    const idents = tokens?.filter((tok) => tok.kind === "ident") ?? [];
    assert.deepStrictEqual(
      idents.map((tok) => tok.text),
      ["p", "dirName"]
    );
  });

  it("returns null for lines it cannot tokenize self-contained (open string/template)", () => {
    assert.strictEqual(tokenizeLine("const s = `spans"), null);
    assert.strictEqual(tokenizeLine('const s = "unterminated'), null);
  });

  it("treats regex literals as opaque", () => {
    const tokens = tokenizeLine("const re = /abc|def/g;");
    const idents = tokens?.filter((tok) => tok.kind === "ident") ?? [];
    assert.deepStrictEqual(
      idents.map((tok) => tok.text),
      ["re"]
    );
  });
});

describe("reconcileDiffNoise — genuine changes are never touched", () => {
  it("skips the getTempDirPath case: body drift + arg-count change (brief's negative)", () => {
    // v120 renamed getTempDirectory → getTempDirPath AND folded the
    // "claude" subdir into it: the decl hunk contains a norm-dirty body
    // line and the call-site hunk drops an argument. Neither hunk may
    // produce a candidate, even with the descriptive tier enabled.
    const prior = `
      function getTempDirectory() {
        return joinPath(tmpRoot());
      }
      function setup(sessionId) {
        let sessionDebugLogPath;
        sessionDebugLogPath = pathLib14.join(getTempDirectory(), "claude", \`d\${sessionId}.log\`);
        return sessionDebugLogPath;
      }
    `;
    const newer = `
      function getTempDirPath() {
        return joinPath(tmpRoot(), "claude");
      }
      function setup(sessionId) {
        let sessionDebugLogPath;
        sessionDebugLogPath = pathLib14.join(getTempDirPath(), \`d\${sessionId}.log\`);
        return sessionDebugLogPath;
      }
    `;
    const { result, output, newText } = run(prior, newer, {
      apply: true,
      descriptiveTier: true
    });
    assert.deepStrictEqual(result.renames, []);
    assert.strictEqual(
      output,
      newText,
      "genuine change must leave text untouched"
    );
    assert.strictEqual(result.hunks.genuine, 2);
    assert.strictEqual(result.hunks.noise, 0);
  });

  it("keeps keyword changes (return vs throw) in the diff", () => {
    const prior = `function f(x) { return x; }`;
    const newer = `function f(x) { throw x; }`;
    const { result } = run(prior, newer, {
      apply: true,
      descriptiveTier: true
    });
    assert.deepStrictEqual(result.renames, []);
    assert.ok(result.hunks.genuine >= 1);
  });

  it("keeps string content changes in the diff", () => {
    const prior = `console.log("hello old world");`;
    const newer = `console.log("hello new world");`;
    const { result } = run(prior, newer, {
      apply: true,
      descriptiveTier: true
    });
    assert.deepStrictEqual(result.renames, []);
    assert.ok(result.hunks.genuine >= 1);
  });

  it("taints hunks whose differing token is a property name", () => {
    // obj.fooMethodOld() vs obj.fooMethodNew(): both tokens are identifier
    // tokens so the norm gate passes, but the position is a member
    // property — a genuine behavioral change. Resolution must taint the
    // hunk and produce nothing.
    const prior = `
      const obj = makeObj();
      obj.fooMethodOld();
    `;
    const newer = `
      const obj = makeObj();
      obj.fooMethodNew();
    `;
    const { result, output, newText } = run(prior, newer, {
      apply: true,
      descriptiveTier: true
    });
    assert.deepStrictEqual(result.renames, []);
    assert.strictEqual(output, newText);
    assert.strictEqual(result.hunks.tainted, 1);
  });

  it("taints property positions even when a same-named binding is in scope", () => {
    // `obj.status` vs `obj.currentStatus`: the property token collides with
    // the local binding `status`. scope.getBinding() finds the binding, but
    // the position is NOT one of its occurrences — the property hunk must
    // taint (a genuine property difference), while the binding's own decl
    // and reference hunks still reconcile it.
    const prior = `
      function f() {
        var currentStatus = init();
        console.log("a");
        track(obj.currentStatus);
        console.log("b");
        return currentStatus;
      }
    `;
    const newer = `
      function f() {
        var status = init();
        console.log("a");
        track(obj.status);
        console.log("b");
        return status;
      }
    `;
    const { result, output } = run(prior, newer, {
      apply: true,
      descriptiveTier: true
    });
    assert.strictEqual(result.hunks.tainted, 1);
    assert.strictEqual(result.renames.length, 1);
    assert.strictEqual(result.renames[0].toName, "currentStatus");
    assert.ok(
      output.includes("track(obj.status)"),
      "the property access must keep its own (differing) name"
    );
  });

  it("taints hunks whose differing token is an unbound (free) identifier", () => {
    const prior = `state = globalRegistryOld;`;
    const newer = `state = globalRegistryNew;`;
    const { result } = run(prior, newer, {
      apply: true,
      descriptiveTier: true
    });
    assert.deepStrictEqual(result.renames, []);
    assert.strictEqual(result.hunks.tainted, 1);
  });

  it("taints destructuring-shorthand positions (key and binding share the token)", () => {
    // `const { name } = x` — the shorthand token is simultaneously the
    // property KEY (possible genuine change) and the binding. Conservatively
    // taint; the reference votes alone must not rename (the declaration
    // evidence is ambiguous).
    const prior = `
      function f(opts) {
        const { sessionReconnectTimestamp } = opts;
        return sessionReconnectTimestamp;
      }
    `;
    const newer = `
      function f(opts) {
        const { sessionStartTime } = opts;
        return sessionStartTime;
      }
    `;
    const { result, output, newText } = run(prior, newer, {
      apply: true,
      descriptiveTier: true
    });
    assert.deepStrictEqual(result.renames, []);
    assert.strictEqual(output, newText);
    assert.strictEqual(result.hunks.tainted, 1);
  });

  it("skips noise hunks larger than maxHunkLines", () => {
    const mk = (names: string[]) =>
      `function f() {\n${names
        .map((n, i) => `  var ${n} = load${i}();`)
        .join("\n")}\n}`;
    const priorNames = Array.from({ length: 12 }, (_, i) => `priorName${i}Val`);
    const newNames = Array.from({ length: 12 }, (_, i) => `newName${i}Val`);
    const { result } = run(mk(priorNames), mk(newNames), {
      apply: true,
      descriptiveTier: true,
      maxHunkLines: 10
    });
    assert.deepStrictEqual(result.renames, []);
    assert.strictEqual(result.hunks.oversized, 1);
  });
});

describe("reconcileDiffNoise — asymmetric tier (minified → descriptive)", () => {
  const priorLeg = `
    function setup() {
      var completionState = loadState();
      console.log("step one");
      if (completionState.ready) {
        console.log("step two");
      }
      return completionState;
    }
    function loadState() {
      return { ready: true };
    }
  `;
  const newLeg = `
    function setup() {
      var Tj_ = loadState();
      console.log("step one");
      if (Tj_.ready) {
        console.log("step two");
      }
      return Tj_;
    }
    function loadState() {
      return { ready: true };
    }
  `;

  it("dry-run dumps the candidate without mutating the AST", () => {
    const { result, output, newText } = run(priorLeg, newLeg);
    assert.strictEqual(result.renames.length, 1);
    const [candidate] = result.renames;
    assert.strictEqual(candidate.fromName, "Tj_");
    assert.strictEqual(candidate.toName, "completionState");
    assert.strictEqual(candidate.kind, "asymmetric");
    assert.strictEqual(candidate.votes, 3);
    assert.strictEqual(candidate.applied, false);
    assert.strictEqual(output, newText, "dry-run must not mutate");
  });

  it("apply renames the binding once and the whole diff collapses", () => {
    const { result, output, priorText } = run(priorLeg, newLeg, {
      apply: true
    });
    assert.strictEqual(result.renames.length, 1);
    assert.strictEqual(result.renames[0].applied, true);
    assert.strictEqual(
      output,
      priorText,
      "every occurrence (decl + refs) must snap to the prior name"
    );
  });

  it("renames identifiers referenced inside template expressions", () => {
    const prior = `
      function f() {
        var logDirectory = base();
        return \`p \${logDirectory} s\`;
      }
    `;
    const newer = `
      function f() {
        var Qx_ = base();
        return \`p \${Qx_} s\`;
      }
    `;
    const { result, output, priorText } = run(prior, newer, { apply: true });
    assert.strictEqual(result.renames.length, 1);
    assert.strictEqual(result.renames[0].toName, "logDirectory");
    assert.strictEqual(output, priorText);
  });

  it("treats the same minified token in different scopes as independent bindings", () => {
    const prior = `
      function a() {
        var parsedConfig = one();
        return parsedConfig;
      }
      function b() {
        var mergedOptions = two();
        return mergedOptions;
      }
    `;
    const newer = `
      function a() {
        var Tj_ = one();
        return Tj_;
      }
      function b() {
        var Tj_ = two();
        return Tj_;
      }
    `;
    const { result, output, priorText } = run(prior, newer, { apply: true });
    assert.strictEqual(result.renames.length, 2);
    const names = result.renames.map((r) => r.toName).sort();
    assert.deepStrictEqual(names, ["mergedOptions", "parsedConfig"]);
    assert.strictEqual(output, priorText);
  });

  it("requires unanimous agreement across a binding's occurrences", () => {
    // The two hunks vote for different prior names for the same binding —
    // an alignment slip. The binding must be skipped.
    const prior = `
      function f() {
        var alphaResult = one();
        console.log("mid");
        return betaResult;
      }
    `;
    const newer = `
      function f() {
        var Tj_ = one();
        console.log("mid");
        return Tj_;
      }
    `;
    const { result, output, newText } = run(prior, newer, { apply: true });
    assert.deepStrictEqual(result.renames, []);
    assert.ok(skipReasons(result).includes("disagreement"));
    assert.strictEqual(output, newText);
  });

  it("vetoes bindings with occurrences on unchanged lines (would create new hunks)", () => {
    // The new leg redeclares Tj_ (one binding); the prior leg has two
    // separate bindings. Lines 3 and 6 are byte-identical across legs, so
    // renaming the new Tj_ would turn unchanged lines into fresh diff
    // hunks — and the vote evidence is an alignment artifact. Skip.
    const prior = `
      function f() {
        var completionState = one();
        var Tj_ = two();
        console.log("mid");
        use(completionState);
        return Tj_;
      }
    `;
    const newer = `
      function f() {
        var Tj_ = one();
        var Tj_ = two();
        console.log("mid");
        use(Tj_);
        return Tj_;
      }
    `;
    const { result, output, newText } = run(prior, newer, { apply: true });
    assert.deepStrictEqual(result.renames, []);
    assert.ok(skipReasons(result).includes("occurrence-outside-diff"));
    assert.strictEqual(output, newText);
  });

  it("never renames to a minified prior name (reroll)", () => {
    const prior = `
      function f() {
        var $2_ = one();
        return $2_;
      }
    `;
    const newer = `
      function f() {
        var wP_ = one();
        return wP_;
      }
    `;
    const { result, output, newText } = run(prior, newer, { apply: true });
    assert.deepStrictEqual(result.renames, []);
    assert.ok(skipReasons(result).includes("reroll"));
    assert.strictEqual(output, newText);
  });

  it("never downgrades a descriptive new name to a minified prior name", () => {
    const prior = `
      function f() {
        var Tj_ = one();
        return Tj_;
      }
    `;
    const newer = `
      function f() {
        var completionState = one();
        return completionState;
      }
    `;
    const { result, output, newText } = run(prior, newer, { apply: true });
    assert.deepStrictEqual(result.renames, []);
    assert.ok(skipReasons(result).includes("name-downgrade"));
    assert.strictEqual(output, newText);
  });

  it("skips skip-listed (ineligible) new names", () => {
    const prior = `
      function f() {
        var moduleLoader = one();
        return moduleLoader;
      }
    `;
    const newer = `
      function f() {
        var __wq1 = one();
        return __wq1;
      }
    `;
    const { result } = run(prior, newer, { apply: true });
    assert.deepStrictEqual(result.renames, []);
    assert.ok(skipReasons(result).includes("not-eligible"));
  });

  it("reconciles multi-line declaration blocks (Tier 2)", () => {
    const prior = `
      var fileSystem33 = requireShim("fs");
      var pathHelper = requireShim("path");
      var operatingSystem = requireShim("os");
      var childProcessLib = requireShim("child_process");
      console.log("anchor");
    `;
    const newer = `
      var Kq_ = requireShim("fs");
      var Zx1 = requireShim("path");
      var Wm$ = requireShim("os");
      var Hn2 = requireShim("child_process");
      console.log("anchor");
    `;
    const { result, output, priorText } = run(prior, newer, { apply: true });
    assert.strictEqual(result.renames.length, 4);
    assert.strictEqual(output, priorText);
  });
});

describe("reconcileDiffNoise — collisions and apply ordering", () => {
  // New leg already binds `completionState` (a different binding than the
  // one the prior named that way), so Tj_ → completionState collides.
  const priorLeg = `
    function f() {
      var completionState = one();
      console.log("mid");
      var completionState2 = other();
      return completionState;
    }
  `;
  const newLeg = `
    function f() {
      var Tj_ = one();
      console.log("mid");
      var completionState = other();
      return Tj_;
    }
  `;

  it("rejects a rename whose target is already bound in scope", () => {
    const { result } = run(priorLeg, newLeg, { apply: true });
    assert.deepStrictEqual(result.renames, []);
    assert.ok(
      skipReasons(result).includes("rename-rejected:target-in-scope"),
      `expected target-in-scope rejection, got: ${skipReasons(result).join(", ")}`
    );
  });

  it("retry loop: a blocked rename applies after the blocker moves away", () => {
    // With the descriptive tier on, completionState → completionState2
    // frees the name, and the retry loop then lands Tj_ → completionState.
    const { result, output, priorText } = run(priorLeg, newLeg, {
      apply: true,
      descriptiveTier: true
    });
    assert.strictEqual(result.renames.filter((r) => r.applied).length, 2);
    assert.strictEqual(output, priorText);
  });

  it("decl-clean fixpoint: a reconciled dependency cleans its dependents' declarations", () => {
    // queryText's declaration differs in TWO positions (its own name and
    // the callee), so it is not clean on its own. But the callee pair
    // buildQueryText → buildQueryString reconciles via its own clean
    // declaration, and once applied the dependent declaration differs only
    // in its own name — the brief's "already reconciled (same binding)"
    // condition. Both must land; output converges to the prior text.
    const prior = `
      function f(input) {
        let buildQueryString = makeBuilder();
        console.log("a");
        let queryString = buildQueryString(input);
        return queryString;
      }
    `;
    const newer = `
      function f(input) {
        let buildQueryText = makeBuilder();
        console.log("a");
        let queryText = buildQueryText(input);
        return queryText;
      }
    `;
    const { result, output, priorText } = run(prior, newer, {
      apply: true,
      descriptiveTier: true
    });
    assert.strictEqual(result.renames.filter((r) => r.applied).length, 2);
    assert.strictEqual(output, priorText);
  });

  it("decl-clean fixpoint never bootstraps from an unreconciled dependency", () => {
    // Same shape, but the callee's own declaration hunk is genuine (bodies
    // drifted), so the callee never reconciles — and the dependent's
    // declaration must stay unclean. Nothing may rename.
    const prior = `
      function buildQueryString() {
        return stored.template + suffix();
      }
      function f(input) {
        let queryString = buildQueryString(input);
        return queryString;
      }
    `;
    const newer = `
      function buildQueryText() {
        return prefix();
      }
      function f(input) {
        let queryText = buildQueryText(input);
        return queryText;
      }
    `;
    const { result, output, newText } = run(prior, newer, {
      apply: true,
      descriptiveTier: true
    });
    assert.deepStrictEqual(result.renames, []);
    assert.ok(skipReasons(result).includes("decl-not-clean"));
    assert.strictEqual(output, newText);
  });
});

describe("reconcileDiffNoise — descriptive tier (transfer-gap)", () => {
  const priorLeg = `
    function createSession(id) {
      let sessionReconnectTimestamp = now();
      console.log("anchor one");
      schedule(id, sessionReconnectTimestamp, id);
      console.log("anchor two");
      return sessionReconnectTimestamp;
    }
  `;
  const newLeg = `
    function createSession(id) {
      let sessionStartTime = now();
      console.log("anchor one");
      schedule(id, sessionStartTime, id);
      console.log("anchor two");
      return sessionStartTime;
    }
  `;

  it("is disabled by default — descriptive candidates are reported skipped", () => {
    const { result, output, newText } = run(priorLeg, newLeg, { apply: true });
    assert.deepStrictEqual(result.renames, []);
    assert.ok(skipReasons(result).includes("descriptive-tier-disabled"));
    assert.strictEqual(output, newText);
  });

  it("reference sites resolve to the binding and the rename applies once", () => {
    const { result, output, priorText } = run(priorLeg, newLeg, {
      apply: true,
      descriptiveTier: true
    });
    assert.strictEqual(result.renames.length, 1);
    const [rename] = result.renames;
    assert.strictEqual(rename.fromName, "sessionStartTime");
    assert.strictEqual(rename.toName, "sessionReconnectTimestamp");
    assert.strictEqual(rename.kind, "descriptive");
    assert.strictEqual(rename.votes, 3);
    assert.strictEqual(output, priorText, "decl + call site + return all snap");
  });

  it("requires a clean declaration: init drift (different callee) blocks the rename", () => {
    // `let sessionStartTime = getStartTime()` vs
    // `let sessionReconnectTimestamp = getReconnectTime()` — the decl line
    // has TWO differing identifier positions, so the value may be computed
    // differently (the brief's trap). The callee pair is blocked too: its
    // own declaration hunk is genuine (bodies differ).
    const prior = `
      function getReconnectTime() {
        return stored.reconnect + offset();
      }
      function make(id) {
        let sessionReconnectTimestamp = getReconnectTime();
        console.log("anchor");
        return sessionReconnectTimestamp;
      }
    `;
    const newer = `
      function getStartTime() {
        return clockNow();
      }
      function make(id) {
        let sessionStartTime = getStartTime();
        console.log("anchor");
        return sessionStartTime;
      }
    `;
    const { result, output, newText } = run(prior, newer, {
      apply: true,
      descriptiveTier: true
    });
    assert.deepStrictEqual(result.renames, []);
    assert.ok(skipReasons(result).includes("decl-not-clean"));
    assert.strictEqual(output, newText);
  });

  it("param renames pass when the signature line differs only in that name", () => {
    const prior = `
      function stringify(inputObject, indentWidth) {
        return serialize(inputObject, indentWidth);
      }
    `;
    const newer = `
      function stringify(dataObject, indentWidth) {
        return serialize(dataObject, indentWidth);
      }
    `;
    const { result, output, priorText } = run(prior, newer, {
      apply: true,
      descriptiveTier: true
    });
    assert.strictEqual(result.renames.length, 1);
    assert.strictEqual(result.renames[0].toName, "inputObject");
    assert.strictEqual(output, priorText);
  });
});

describe("reconcileDiffNoise — review hardening", () => {
  it("taints when the differing token is a property inside the binding's own write", () => {
    // `accountId = cfg.accountId;` — the assignment is a constantViolation
    // of the binding, and the RHS property token shares its name. The
    // property position must taint (genuine property change), and with the
    // write's provenance tainted the decl+return votes must NOT rename —
    // the value is genuinely computed from a different property now.
    const prior = `
      function f(cfg) {
        let userId;
        userId = cfg.userId;
        return userId;
      }
    `;
    const newer = `
      function f(cfg) {
        let accountId;
        accountId = cfg.accountId;
        return accountId;
      }
    `;
    const { result, output, newText } = run(prior, newer, {
      apply: true,
      descriptiveTier: true
    });
    assert.deepStrictEqual(result.renames, []);
    assert.strictEqual(output, newText);
    assert.ok(result.hunks.tainted >= 1, "property-in-write must taint");
  });

  it("requires the declaration to sit in a clean aligned pair (all tiers)", () => {
    // The decl's own hunk is genuine (init genuinely changed), and only a
    // lone `return X;` reference pairs cleanly. Identity is in doubt —
    // renaming would pin a descriptive prior name onto a differently-
    // computed value. Must skip, asymmetric tier included.
    const prior = `
      function f() {
        var completionState = makeThing(1, 2);
        console.log("mid");
        return completionState;
      }
    `;
    const newer = `
      function f() {
        var Tj_ = totallyDifferentInit() + extra();
        console.log("mid");
        return Tj_;
      }
    `;
    const { result, output, newText } = run(prior, newer, { apply: true });
    assert.deepStrictEqual(result.renames, []);
    assert.ok(skipReasons(result).includes("decl-not-aligned"));
    assert.strictEqual(output, newText);
  });

  it("skips export-involved bindings (Babel's renamer would split the declaration)", () => {
    const prior = `
      export const completionState = one();
      console.log(completionState);
    `;
    const newer = `
      export const Tj_ = one();
      console.log(Tj_);
    `;
    const { result, output, newText } = run(prior, newer, { apply: true });
    assert.deepStrictEqual(result.renames, []);
    assert.ok(skipReasons(result).includes("export-involved"));
    assert.strictEqual(output, newText);
  });

  it("routes non-minted names (ALL_CAPS) through the descriptive gate, not asymmetric", () => {
    // HTTP_STATUS is a deliberate SCREAMING_CASE name, not a Bun-minted
    // token, so it must NOT get the weaker asymmetric gate. Its
    // declaration's init calls a bound helper whose own body drifted
    // (genuine, never reconciled), so the descriptive decl-clean gate
    // blocks the rename — proving ALL_CAPS took the descriptive path.
    const prior = `
      function computeCodes() { return baseTable.codes; }
      function f() {
        var RESPONSE_CODES = computeCodes();
        return RESPONSE_CODES;
      }
    `;
    const newer = `
      function computeStatus() { return liveFeed.status; }
      function f() {
        var HTTP_STATUS = computeStatus();
        return HTTP_STATUS;
      }
    `;
    const { result } = run(prior, newer, {
      apply: true,
      descriptiveTier: true
    });
    assert.deepStrictEqual(
      result.renames.filter((r) => r.applied),
      []
    );
    assert.ok(
      skipReasons(result).includes("decl-not-clean"),
      `ALL_CAPS must need descriptive evidence, got: ${skipReasons(result).join(", ")}`
    );
  });

  it("restores an ALL_CAPS prior name when the declaration is clean", () => {
    const prior = `
      function f() {
        var RESPONSE_CODES = buildTable();
        return RESPONSE_CODES;
      }
    `;
    const newer = `
      function f() {
        var HTTP_STATUS = buildTable();
        return HTTP_STATUS;
      }
    `;
    const { result, output, priorText } = run(prior, newer, {
      apply: true,
      descriptiveTier: true
    });
    assert.strictEqual(result.renames.length, 1);
    assert.strictEqual(result.renames[0].toName, "RESPONSE_CODES");
    assert.strictEqual(result.renames[0].kind, "descriptive");
    assert.strictEqual(output, priorText);
  });

  it("skips everything when the prior text is too dissimilar (corpus gate)", () => {
    // The multi-file-unpack failure mode: one coincidentally clean line
    // pair (`var X = req(1);`) between two otherwise unrelated programs.
    // With the shared-lineage premise broken, aligned pairs are
    // coincidence, so the pass must abstain entirely rather than snap a
    // name from unrelated code.
    const prior = `
      var configLoader = req(1);
      alpha(1);
      beta(2);
      gamma(3);
      delta(4);
      epsilon(5);
      zeta(6);
      eta(7);
      theta(8);
      iota(9);
    `;
    const newer = `
      var a1_ = req(1);
      one.thing("x");
      two.other("y");
      three.next("z");
      four.more("w");
      five.calls("v");
      six.done("u");
      seven.went("t");
      eight.gone("s");
      nine.here("r");
    `;
    const priorLineCount = canon(prior).split("\n").length;
    const { result, output, newText } = run(prior, newer, {
      apply: true,
      descriptiveTier: true,
      priorLineCount
    });
    assert.deepStrictEqual(result.renames, []);
    assert.strictEqual(output, newText);
    assert.ok(result.priorTooDissimilar, "corpus gate must report the skip");
  });

  it("does NOT abstain when the prior is large and mostly unchanged", () => {
    const shared = Array.from({ length: 12 }, (_, i) => `anchor${i}();`).join(
      "\n"
    );
    const prior = `
      var completionState = load();
      ${shared}
      use(completionState);
    `;
    const newer = `
      var Tj_ = load();
      ${shared}
      use(Tj_);
    `;
    const priorLineCount = canon(prior).split("\n").length;
    const { result, output, priorText } = run(prior, newer, {
      apply: true,
      priorLineCount
    });
    assert.ok(!result.priorTooDissimilar);
    assert.strictEqual(result.renames.length, 1);
    assert.strictEqual(output, priorText);
  });
});

describe("reconcileDiffNoise — eval/with taint", () => {
  it("freezes module-level bindings when a direct eval exists", () => {
    // eval("x") can resolve any module binding by its ORIGINAL name at
    // runtime; renaming Tj_ would change behavior while parsing cleanly.
    const prior = `
      var completionState = one();
      eval("x");
      console.log(completionState);
    `;
    const newer = `
      var Tj_ = one();
      eval("x");
      console.log(Tj_);
    `;
    const { result, output, newText } = run(prior, newer, { apply: true });
    assert.deepStrictEqual(result.renames, []);
    assert.ok(skipReasons(result).includes("eval-taint-frozen"));
    assert.strictEqual(output, newText);
  });

  it("freezes locals of functions enclosing an eval site", () => {
    const prior = `
      function f() {
        var completionState = one();
        eval("x");
        return completionState;
      }
    `;
    const newer = `
      function f() {
        var Tj_ = one();
        eval("x");
        return Tj_;
      }
    `;
    const { result } = run(prior, newer, { apply: true });
    assert.deepStrictEqual(result.renames, []);
    assert.ok(skipReasons(result).includes("eval-taint-frozen"));
  });

  it("still renames locals of functions off the eval scope chain", () => {
    const prior = `
      eval("x");
      function f() {
        var completionState = one();
        return completionState;
      }
    `;
    const newer = `
      eval("x");
      function f() {
        var Tj_ = one();
        return Tj_;
      }
    `;
    const { result, output, priorText } = run(prior, newer, { apply: true });
    assert.strictEqual(result.renames.length, 1);
    assert.strictEqual(output, priorText);
  });
});

describe("reconcileDiffNoise — determinism", () => {
  it("dry-run twice produces identical results", () => {
    const prior = `
      function f() {
        var completionState = one();
        return completionState;
      }
    `;
    const newer = `
      function f() {
        var Tj_ = one();
        return Tj_;
      }
    `;
    const a = run(prior, newer);
    const b = run(prior, newer);
    assert.deepStrictEqual(a.result, b.result);
  });
});

describe("reconcileDiffNoise — consumer tier (changed-leaf inheritance)", () => {
  /** Run with the consumer tier armed the way reconcile-step wires it. */
  function runConsumer(
    priorRaw: string,
    newRaw: string,
    opts: Partial<ReconcileOptions> = {}
  ) {
    const priorText = canon(priorRaw);
    return {
      ...run(priorRaw, newRaw, {
        apply: true,
        descriptiveTier: true,
        consumerTier: true,
        priorNames: collectWordTokens(priorText),
        ...opts
      })
    };
  }

  // A changed leaf: the fn's body drifted (async now) so its declaration
  // hunk is genuine — the aligned-declaration proof can never hold — and
  // the fresh leg minted a new head name. Its UNCHANGED callers testify
  // from clean rename-noise hunks that it plays the same role.
  const priorLeaf = `
    function loadConfig(path) {
      const parsed = readFile(path);
      return parsed.settings;
    }
    function readerOne(ctx) {
      if (ctx.ready) {
        return loadConfig(ctx.path);
      }
      return null;
    }
    function readerTwo(list) {
      return list.map((entry) => loadConfig(entry));
    }
  `;
  const newLeaf = `
    async function fetchConfigData(path) {
      const parsed = await readFile(path);
      return parsed.settings;
    }
    function readerOne(ctx) {
      if (ctx.ready) {
        return fetchConfigData(ctx.path);
      }
      return null;
    }
    function readerTwo(list) {
      return list.map((entry) => fetchConfigData(entry));
    }
  `;

  it("inherits a changed leaf's prior name from two caller witnesses in distinct hunks", () => {
    const { result, output } = runConsumer(priorLeaf, newLeaf);
    const consumer = result.renames.find((r) => r.kind === "consumer");
    assert.ok(
      consumer,
      `expected a consumer rename, skips: ${skipReasons(result).join(",")}`
    );
    assert.strictEqual(consumer.fromName, "fetchConfigData");
    assert.strictEqual(consumer.toName, "loadConfig");
    assert.ok(output.includes("loadConfig("), "prior name restored");
    assert.ok(!output.includes("fetchConfigData"), "fresh mint gone");
    assert.ok(output.includes("async function loadConfig"), "real change kept");
  });

  it("abstains without the consumer tier flag (decl-not-aligned as before)", () => {
    const { result } = run(priorLeaf, newLeaf, {
      apply: true,
      descriptiveTier: true
    });
    assert.ok(!result.renames.some((r) => r.kind === "consumer"));
    assert.ok(skipReasons(result).includes("decl-not-aligned"));
  });

  it("abstains on a single caller witness", () => {
    const priorOne = `
      function loadConfig(path) {
        const parsed = readFile(path);
        return parsed.settings;
      }
      function readerOne(ctx) {
        if (ctx.ready) {
          return loadConfig(ctx.path);
        }
        return null;
      }
    `;
    const newOne = `
      async function fetchConfigData(path) {
        const parsed = await readFile(path);
        return parsed.settings;
      }
      function readerOne(ctx) {
        if (ctx.ready) {
          return fetchConfigData(ctx.path);
        }
        return null;
      }
    `;
    const { result, output } = runConsumer(priorOne, newOne);
    assert.ok(!result.renames.some((r) => r.kind === "consumer"));
    assert.ok(skipReasons(result).includes("consumer-single-hunk"));
    assert.ok(output.includes("fetchConfigData"));
  });

  it("abstains when both witnesses sit in one hunk", () => {
    const priorAdjacent = `
      function loadConfig(path) {
        const parsed = readFile(path);
        return parsed.settings;
      }
      function readerOne(a, b) {
        const first = loadConfig(a);
        const second = loadConfig(b);
        return [first, second];
      }
    `;
    const newAdjacent = `
      async function fetchConfigData(path) {
        const parsed = await readFile(path);
        return parsed.settings;
      }
      function readerOne(a, b) {
        const first = fetchConfigData(a);
        const second = fetchConfigData(b);
        return [first, second];
      }
    `;
    const { result } = runConsumer(priorAdjacent, newAdjacent);
    assert.ok(!result.renames.some((r) => r.kind === "consumer"));
    assert.ok(skipReasons(result).includes("consumer-single-hunk"));
  });

  it("abstains when the prior name is still live in the new output", () => {
    const priorLive = `${priorLeaf}
      var keeper = 1;
    `;
    const newLive = `${newLeaf}
      var loadConfig = 1;
    `;
    const { result } = runConsumer(priorLive, newLive);
    assert.ok(!result.renames.some((r) => r.kind === "consumer"));
    assert.ok(skipReasons(result).includes("consumer-to-name-live"));
  });

  it("abstains when the fresh name is not novel this hop", () => {
    const priorStale = `${priorLeaf}
      var fetchConfigData = 1;
    `;
    const newStale = `${newLeaf}
      var fetchConfigData2 = 1;
    `;
    const { result } = runConsumer(priorStale, newStale);
    assert.ok(!result.renames.some((r) => r.kind === "consumer"));
    assert.ok(skipReasons(result).includes("consumer-from-not-novel"));
  });
});

describe("reconcileDiffNoise — strategy trail", () => {
  it("records applied restores and dry-run records nothing", () => {
    const priorLeg = `
      function setup() {
        var completionState = loadState();
        console.log("step one");
        return completionState;
      }
      function loadState() {
        return { ready: true };
      }
    `;
    const newLeg = `
      function setup() {
        var Tj_ = loadState();
        console.log("step one");
        return Tj_;
      }
      function loadState() {
        return { ready: true };
      }
    `;
    strategyTrail.reset(true);
    try {
      run(priorLeg, newLeg);
      assert.strictEqual(
        strategyTrail.report().trails.length,
        0,
        "dry-run must not record"
      );
      run(priorLeg, newLeg, { apply: true });
      const { funnel, trails } = strategyTrail.report();
      assert.strictEqual(funnel["reconcile-asymmetric"].applied, 1);
      const entry = trails.find((e) => e.terminalBy === "reconcile-asymmetric");
      assert.ok(entry, "restored binding carries a trail entry");
      assert.strictEqual(entry.oldName, "Tj_");
    } finally {
      strategyTrail.reset(false);
    }
  });
});

describe("reconcileDiffNoise — half-mint restore gate", () => {
  // A prior half-mint fossil (T7Class) must never overwrite a DESCRIPTIVE
  // fresh name — the fresh LLM name is strictly better. The asymmetric
  // tier (minted fresh name) still restores: the fossil at least carries
  // a word, and the coverage sweep re-names it afterwards.
  const priorLeg = `
    function setup() {
      var T7Class = loadState();
      console.log("step one");
      return T7Class;
    }
    function loadState() {
      return { ready: true };
    }
  `;

  it("refuses the fossil over a descriptive fresh name", () => {
    const newLeg = `
      function setup() {
        var InputInstance = loadState();
        console.log("step one");
        return InputInstance;
      }
      function loadState() {
        return { ready: true };
      }
    `;
    const { result, output, newText } = run(priorLeg, newLeg, {
      apply: true,
      descriptiveTier: true
    });
    assert.deepStrictEqual(result.renames, []);
    assert.ok(
      skipReasons(result).includes("half-mint-restore"),
      `expected half-mint-restore skip, got ${JSON.stringify(result.skipped)}`
    );
    assert.strictEqual(output, newText, "fresh descriptive name must stay");
  });

  it("still restores the fossil over a minted fresh name (asymmetric)", () => {
    const newLeg = `
      function setup() {
        var Tj_ = loadState();
        console.log("step one");
        return Tj_;
      }
      function loadState() {
        return { ready: true };
      }
    `;
    const { result } = run(priorLeg, newLeg, { apply: true });
    assert.strictEqual(result.renames.length, 1);
    assert.strictEqual(result.renames[0].toName, "T7Class");
    assert.strictEqual(result.renames[0].kind, "asymmetric");
  });
});
