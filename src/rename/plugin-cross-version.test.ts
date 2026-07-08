import assert from "node:assert";
import fs from "node:fs";
import { describe, it } from "node:test";
import type { BatchRenameRequest, LLMProvider } from "../llm/types.js";
import { createRenamePlugin } from "./plugin.js";

/**
 * Cross-version transfer, end to end at the plugin level: humanify v1 of a
 * committed minified fixture, then humanify v2 with v1's output as the
 * prior version. This is the --prior-version workflow in miniature.
 */

const FIXTURES = new URL(
  "../../test/e2e/fixtures/disambiguation/minified/",
  import.meta.url
);

function readFixture(version: string, minifier: string): string {
  return fs.readFileSync(
    new URL(`${version}/${minifier}.js`, FIXTURES),
    "utf-8"
  );
}

/** Provider that counts calls and tags names with a run-specific suffix. */
function countingProvider(suffix: string): {
  provider: LLMProvider;
  calls: () => number;
} {
  let count = 0;
  const provider: LLMProvider = {
    async suggestAllNames(request: BatchRenameRequest) {
      count++;
      const renames: Record<string, string> = {};
      for (const id of request.identifiers) {
        renames[id] = `${id}${suffix}`;
      }
      return { renames };
    }
  };
  return { provider, calls: () => count };
}

describe("cross-version prior-version transfer (bun fixture pair)", () => {
  it("reuses v1 names in v2, reduces LLM calls, and produces parseable output", async () => {
    const v1Code = readFixture("v1.0.0", "bun-default");
    const v2Code = readFixture("v1.1.0", "bun-default");

    // Run A: fresh humanify of v1
    const runA = countingProvider("Renamed");
    const renameV1 = createRenamePlugin({ provider: runA.provider });
    const resultV1 = await renameV1(v1Code);

    assert.strictEqual(
      resultV1.parseFailure,
      undefined,
      `v1 output must parse: ${resultV1.parseFailure?.message}`
    );
    assert.ok(runA.calls() > 0, "fresh run should make LLM calls");

    // Run B: humanify v2 with v1's humanified output as prior version.
    // A different suffix means any "Renamed" identifier in v2's output
    // can only have come from the prior-version transfer.
    const runB = countingProvider("Fresh");
    const renameV2 = createRenamePlugin({
      provider: runB.provider,
      priorVersionCode: resultV1.code
    });
    const resultV2 = await renameV2(v2Code);

    assert.strictEqual(
      resultV2.parseFailure,
      undefined,
      `v2 output must parse: ${resultV2.parseFailure?.message}\n${resultV2.parseFailure?.excerpt ?? ""}`
    );

    const transferred = resultV2.code.match(/[A-Za-z0-9_$]*Renamed/g) ?? [];
    assert.ok(
      transferred.length > 0,
      `v2 output should reuse names invented in v1, got:\n${resultV2.code}`
    );
    assert.ok(
      runB.calls() < runA.calls(),
      `prior-version run should need fewer LLM calls (fresh=${runA.calls()}, cached=${runB.calls()})`
    );
  });

  it("does not rename an unrelated module binding from a phantom placeholder pair", async () => {
    // The matched pair q↔loadUsers produces the placeholder pair {e→user}
    // for the NESTED arrow's param. The module binding `e` is unrelated —
    // the outer function never references it. A name-string vote would
    // rename it to `user`; binding-identity voting must not.
    const priorCode = `
      function loadUsers(list) {
        return list.map(user => user.id);
      }
      console.log(loadUsers);
    `;
    const v2Code = `
      var e = 99;
      function q(n) {
        return n.map(e => e.id);
      }
      console.log(q, e);
    `;

    const run = countingProvider("Fresh");
    const rename = createRenamePlugin({
      provider: run.provider,
      priorVersionCode: priorCode
    });
    const result = await rename(v2Code);

    assert.strictEqual(result.parseFailure, undefined);
    assert.ok(
      !/var user = 99/.test(result.code),
      `module binding e must not be renamed via phantom vote, got:\n${result.code}`
    );
  });

  it("transfers a module binding name when two matched functions vote for it", async () => {
    // `t` is read-only with no initializer — unhashable, so the binding
    // cascade cannot match it; only vote propagation can. Two exact-matched
    // functions reference it, giving two agreeing votes.
    const priorCode = `
      var appConfig;
      function readA() {
        for (let i = 0; i < 3; i++) { if (appConfig > i) console.log(i); }
        return appConfig;
      }
      function readB(x) {
        return x + appConfig;
      }
    `;
    const v2Code = `
      var t;
      function rA() {
        for (let i = 0; i < 3; i++) { if (t > i) console.log(i); }
        return t;
      }
      function rB(x) {
        return x + t;
      }
    `;

    const run = countingProvider("Fresh");
    const rename = createRenamePlugin({
      provider: run.provider,
      priorVersionCode: priorCode
    });
    const result = await rename(v2Code);

    assert.strictEqual(result.parseFailure, undefined);
    assert.ok(
      /var appConfig/.test(result.code),
      `two agreeing votes should transfer the binding name, got:\n${result.code}`
    );
  });

  it("does not transfer a module binding name on a single vote", async () => {
    const priorCode = `
      var appConfig;
      function readA() {
        for (let i = 0; i < 3; i++) { if (appConfig > i) console.log(i); }
        return appConfig;
      }
    `;
    const v2Code = `
      var t;
      function rA() {
        for (let i = 0; i < 3; i++) { if (t > i) console.log(i); }
        return t;
      }
    `;

    const run = countingProvider("Fresh");
    const rename = createRenamePlugin({
      provider: run.provider,
      priorVersionCode: priorCode
    });
    const result = await rename(v2Code);

    assert.strictEqual(result.parseFailure, undefined);
    assert.ok(
      !/var appConfig/.test(result.code),
      `a single vote must not rename a module binding, got:\n${result.code}`
    );
  });

  it("transfers close-match body locals mechanically and excludes them from the LLM", async () => {
    // The pair close-matches (one inserted statement); locals declared in
    // content-aligned statements transfer without any LLM involvement.
    const priorCode = `
      function processItems(list) {
        const filtered = list.filter(Boolean);
        const sorted = filtered.sort();
        const first = sorted[0];
        return first + sorted.length;
      }
    `;
    const v2Code = `
      function p(a) {
        const b = a.filter(Boolean);
        console.log("extra", b.length);
        const c = b.sort();
        const d = c[0];
        return d + c.length;
      }
    `;

    const requested = new Set<string>();
    const provider: LLMProvider = {
      async suggestAllNames(request: BatchRenameRequest) {
        const renames: Record<string, string> = {};
        for (const id of request.identifiers) {
          requested.add(id);
          renames[id] = `${id}Fresh`;
        }
        return { renames };
      }
    };

    const rename = createRenamePlugin({
      provider,
      priorVersionCode: priorCode
    });
    const result = await rename(v2Code);

    assert.strictEqual(result.parseFailure, undefined);
    for (const transferredName of ["filtered", "sorted", "first"]) {
      assert.ok(
        result.code.includes(transferredName),
        `body local "${transferredName}" should transfer, got:\n${result.code}`
      );
    }
    for (const minified of ["b", "c", "d"]) {
      assert.ok(
        !requested.has(minified),
        `transferred local "${minified}" must not be sent to the LLM (requested: ${[...requested].join(", ")})`
      );
    }
  });

  it("keeps dependents of cascade-matched bindings schedulable (no phantom edges)", async () => {
    // `q` is cascade-matched to baseConfig and marked done before the
    // processing pass. `w` (new in this version, unmatched) depends on
    // module:q via its initializer. If matched nodes are DELETED from the
    // graph instead of marked done, w's dependency dangles and it is only
    // released by the deadlock force-break — unordered. The scheduler now
    // asserts graph closure at entry, so a phantom edge fails loudly.
    const priorCode = `
      var baseConfig = { port: 8080, host: "x" };
      function readPort() {
        for (let i = 0; i < 3; i++) { if (i > 1) console.log(i); }
        return baseConfig;
      }
      function readHost(x) { return x + baseConfig.host; }
    `;
    const v2Code = `
      var q = { port: 8080, host: "x" };
      var w = q;
      function rp() {
        for (let i = 0; i < 3; i++) { if (i > 1) console.log(i); }
        return q;
      }
      function rh(x) { return x + q.host; }
    `;

    const run = countingProvider("Fresh");
    const rename = createRenamePlugin({
      provider: run.provider,
      priorVersionCode: priorCode
    });
    const result = await rename(v2Code);

    assert.strictEqual(result.parseFailure, undefined);
    assert.ok(
      /var baseConfig/.test(result.code),
      `cascade should transfer the binding name, got:\n${result.code}`
    );
  });

  it("transfers distinct prior names to a function-scope binding and a catch param that share one minified name", async () => {
    // Prior humanified leg named the two bindings distinctly. In the new
    // minified leg Bun reused `K` for BOTH the function-scope binding and
    // the catch param (legal: the catch block never references the outer
    // K). The two are different bindings with different placeholder slots,
    // so each must get ITS OWN prior name back — a name-string-keyed
    // transfer collapses the two pairs and can put the catch binding's
    // name on the function-scope binding.
    const priorCode = `
      function handleRequest(input) {
        let requestPayload = buildPayload(input);
        try {
          sendPayload(requestPayload);
        } catch (errorDetails) {
          reportFailure(errorDetails);
        }
        return requestPayload;
      }
      console.log(handleRequest);
    `;
    const v2Code = `
      function A(b) {
        let K = buildPayload(b);
        try {
          sendPayload(K);
        } catch (K) {
          reportFailure(K);
        }
        return K;
      }
      console.log(A);
    `;

    const run = countingProvider("Fresh");
    const rename = createRenamePlugin({
      provider: run.provider,
      priorVersionCode: priorCode
    });
    const result = await rename(v2Code);

    assert.strictEqual(result.parseFailure, undefined);
    assert.match(
      result.code,
      /let requestPayload = buildPayload/,
      `function-scope binding must get its own prior name, not the catch binding's, got:\n${result.code}`
    );
    assert.match(
      result.code,
      /catch\s*\(errorDetails\)/,
      `catch param must get its prior name, got:\n${result.code}`
    );
  });

  it("is stable when v2 equals v1 (identical input reuses names wholesale)", async () => {
    const v1Code = readFixture("v1.0.0", "bun-default");

    const runA = countingProvider("Renamed");
    const renameV1 = createRenamePlugin({ provider: runA.provider });
    const resultV1 = await renameV1(v1Code);

    const runB = countingProvider("Fresh");
    const renameAgain = createRenamePlugin({
      provider: runB.provider,
      priorVersionCode: resultV1.code
    });
    const resultV2 = await renameAgain(v1Code);

    assert.strictEqual(resultV2.parseFailure, undefined);
    assert.ok(
      !resultV2.code.includes("Fresh"),
      `identical input should transfer every name without fresh LLM naming, got fresh names in:\n${resultV2.code}`
    );
  });
});
