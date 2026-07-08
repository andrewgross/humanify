import assert from "node:assert";
import { describe, it } from "node:test";
import type { BatchRenameRequest, LLMProvider } from "../llm/types.js";
import { createRenamePlugin } from "./plugin.js";

/**
 * Megafunction truncation coverage (experiment 015): when a function's
 * generated code exceeds the LLM code cap, bindings declared past the cap
 * must still be VISIBLE in the code of the request that asks to name them.
 * A real LLM omits (or blind-names) identifiers it cannot see — the honest
 * provider below models that: it only returns names for identifiers that
 * appear in the request's code.
 */

/** Provider that renames ONLY identifiers visible in the shown code. */
function honestProvider(): {
  provider: LLMProvider;
  requests: Array<{ identifiers: string[]; code: string }>;
} {
  const requests: Array<{ identifiers: string[]; code: string }> = [];
  const provider: LLMProvider = {
    async suggestAllNames(request: BatchRenameRequest) {
      requests.push({
        identifiers: [...request.identifiers],
        code: request.code
      });
      const renames: Record<string, string> = {};
      for (const id of request.identifiers) {
        const wordRe = new RegExp(`\\b${id.replace(/[$\\]/g, "\\$&")}\\b`);
        if (wordRe.test(request.code)) {
          renames[id] = `${id}Seen`;
        }
      }
      return { renames };
    }
  };
  return { provider, requests };
}

/** A function whose generated code exceeds the 500-line LLM cap, with
 *  bindings declared past the cap (a var and a catch param). */
function buildMegafunctionSource(): string {
  const filler = Array.from({ length: 520 }, (_, i) => `  pad(${i});`).join(
    "\n"
  );
  return `
function megaFn(q0, q1) {
  var w0 = q0 + q1;
${filler}
  var zz = loadConfig(q0);
  try {
    runTask(zz);
  } catch (e4) {
    reportError(e4, zz);
  }
  return zz;
}
console.log(megaFn);
`;
}

/** A function with eligible bindings spread across ~1,900 generated lines:
 *  40 declarations, each followed by filler, so most fall past the cap and
 *  the binding count (42) exceeds the lane threshold (25). */
function buildSpreadMegafunctionSource(): string {
  const groups = Array.from({ length: 40 }, (_, g) => {
    const filler = Array.from(
      { length: 45 },
      (_, i) => `  pad(${g * 1000 + i});`
    ).join("\n");
    return `  var g${g} = compute(${g});\n${filler}`;
  }).join("\n");
  return `
function spreadFn(p0, p1) {
${groups}
  return p0 + p1;
}
console.log(spreadFn);
`;
}

describe("megafunction truncation coverage", () => {
  it("shows every requested identifier in the request code and renames past-cap bindings", async () => {
    const { provider, requests } = honestProvider();
    const rename = createRenamePlugin({ provider });
    const result = await rename(buildMegafunctionSource());

    assert.strictEqual(
      result.parseFailure,
      undefined,
      `output must parse: ${result.parseFailure?.message}`
    );
    assert.strictEqual(result.semanticFailure, undefined);

    // The core contract: an identifier the LLM is asked to name must be
    // visible in the code it is shown.
    for (const req of requests) {
      const invisible = req.identifiers.filter(
        (id) =>
          !new RegExp(`\\b${id.replace(/[$\\]/g, "\\$&")}\\b`).test(req.code)
      );
      assert.deepStrictEqual(
        invisible,
        [],
        `request asked to name identifiers that are not in the shown code: ` +
          `${invisible.join(", ")} (identifiers: ${req.identifiers.join(", ")}; ` +
          `code is ${req.code.split("\n").length} lines)`
      );
    }

    // The user-visible outcome: bindings declared past the cap get named.
    assert.match(
      result.code,
      /var zzSeen = loadConfig\(q0Seen\)/,
      `past-cap var must be renamed, got:\n${result.code.slice(-400)}`
    );
    assert.match(
      result.code,
      /catch \(e4Seen\)/,
      `past-cap catch param must be renamed, got:\n${result.code.slice(-400)}`
    );
    // Pre-cap bindings keep working as before.
    assert.match(result.code, /var w0Seen = q0Seen \+ q1Seen/);
  });

  it("composes with lanes: spread bindings all visible and renamed", async () => {
    const { provider, requests } = honestProvider();
    const rename = createRenamePlugin({ provider });
    const result = await rename(buildSpreadMegafunctionSource());

    assert.strictEqual(result.parseFailure, undefined);
    assert.strictEqual(result.semanticFailure, undefined);

    for (const req of requests) {
      const invisible = req.identifiers.filter(
        (id) =>
          !new RegExp(`\\b${id.replace(/[$\\]/g, "\\$&")}\\b`).test(req.code)
      );
      assert.deepStrictEqual(
        invisible,
        [],
        `identifiers not visible in their request: ${invisible.join(", ")}`
      );
      // Windowing must respect the prompt budget: batches anchor at most
      // ~10 declarations, so the shown code stays within the cap.
      assert.ok(
        req.code.split("\n").length <= 520,
        `request code must stay near the cap, got ${req.code.split("\n").length} lines`
      );
    }

    for (let g = 0; g < 40; g++) {
      assert.match(
        result.code,
        new RegExp(`var g${g}Seen = compute\\(${g}\\)`),
        `binding g${g} must be renamed`
      );
    }
  });

  it("retries for past-cap identifiers still see their declaration", async () => {
    // Fails the first attempt for one past-cap binding (returns the name
    // unchanged), forcing a retry round; the retry snippet is extracted
    // from the WINDOWED code, so the declaration must still be visible.
    const attempts = new Map<string, number>();
    const requests: Array<{ identifiers: string[]; code: string }> = [];
    const provider: LLMProvider = {
      async suggestAllNames(request: BatchRenameRequest) {
        requests.push({
          identifiers: [...request.identifiers],
          code: request.code
        });
        const renames: Record<string, string> = {};
        for (const id of request.identifiers) {
          const wordRe = new RegExp(`\\b${id.replace(/[$\\]/g, "\\$&")}\\b`);
          if (!wordRe.test(request.code)) continue;
          const n = (attempts.get(id) ?? 0) + 1;
          attempts.set(id, n);
          renames[id] = id === "zz" && n === 1 ? "zz" : `${id}Seen`;
        }
        return { renames };
      }
    };

    const rename = createRenamePlugin({ provider });
    const result = await rename(buildMegafunctionSource());

    assert.strictEqual(result.parseFailure, undefined);
    assert.ok(
      (attempts.get("zz") ?? 0) >= 2,
      `zz must have been retried (attempts: ${attempts.get("zz")})`
    );
    assert.match(
      result.code,
      /var zzSeen = loadConfig\(q0Seen\)/,
      `past-cap var must be renamed on retry, got:\n${result.code.slice(-400)}`
    );
  });
});
