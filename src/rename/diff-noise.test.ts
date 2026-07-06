import assert from "node:assert";
import fs from "node:fs";
import { describe, it } from "node:test";
import type { BatchRenameRequest, LLMProvider } from "../llm/types.js";
import { createRenamePlugin } from "./plugin.js";

/**
 * Hermetic diff-noise guardrail for the cross-version goal: humanify v1
 * fresh, humanify v2 with v1's output as the prior, and line-diff the two
 * outputs. The fixture pair differs by exactly ONE inserted
 * console.log("perturbation") statement, so the humanified diff must
 * contain only that insertion's hunk. Replaced lines that are identical
 * after masking identifiers are RENAME NOISE — names that drifted between
 * runs despite unchanged logic — and must be zero.
 *
 * Runs entirely on a mock provider; guards all three 2026-07-06 plan
 * workstreams (matcher precision, close-match transfer, LLM efficiency)
 * without touching an LLM.
 */

const FIXTURES = new URL(
  "../../test/e2e/fixtures/disambiguation/minified/",
  import.meta.url
);

function readFixture(version: string): string {
  return fs.readFileSync(
    new URL(`${version}/bun-default.js`, FIXTURES),
    "utf-8"
  );
}

function suffixProvider(suffix: string): LLMProvider {
  return {
    async suggestAllNames(request: BatchRenameRequest) {
      const renames: Record<string, string> = {};
      for (const id of request.identifiers) {
        renames[id] = `${id}${suffix}`;
      }
      return { renames };
    }
  };
}

/** The plan's noise classifier: a line with every identifier masked. */
function maskIdentifiers(line: string): string {
  return line.replace(/\b[A-Za-z_$][A-Za-z0-9_$]*\b/g, "#");
}

interface LineDiff {
  /** Paired removed/added lines within a hunk (positional pairing) */
  replacements: Array<[string, string]>;
  addedOnly: string[];
  removedOnly: string[];
}

/** Minimal LCS-based line diff; fixtures are a few hundred lines. */
function diffLines(a: string[], b: string[]): LineDiff {
  const m = a.length;
  const n = b.length;
  const lcs: number[][] = Array.from({ length: m + 1 }, () =>
    new Array(n + 1).fill(0)
  );
  for (let i = m - 1; i >= 0; i--) {
    for (let j = n - 1; j >= 0; j--) {
      lcs[i][j] =
        a[i] === b[j]
          ? lcs[i + 1][j + 1] + 1
          : Math.max(lcs[i + 1][j], lcs[i][j + 1]);
    }
  }

  const replacements: Array<[string, string]> = [];
  const addedOnly: string[] = [];
  const removedOnly: string[] = [];
  let i = 0;
  let j = 0;
  let removedRun: string[] = [];
  let addedRun: string[] = [];
  const flushHunk = () => {
    const paired = Math.min(removedRun.length, addedRun.length);
    for (let k = 0; k < paired; k++) {
      replacements.push([removedRun[k], addedRun[k]]);
    }
    removedOnly.push(...removedRun.slice(paired));
    addedOnly.push(...addedRun.slice(paired));
    removedRun = [];
    addedRun = [];
  };
  while (i < m && j < n) {
    if (a[i] === b[j]) {
      flushHunk();
      i++;
      j++;
    } else if (lcs[i + 1][j] >= lcs[i][j + 1]) {
      removedRun.push(a[i]);
      i++;
    } else {
      addedRun.push(b[j]);
      j++;
    }
  }
  removedRun.push(...a.slice(i));
  addedRun.push(...b.slice(j));
  flushHunk();

  return { replacements, addedOnly, removedOnly };
}

describe("hermetic diff-noise regression (fixture pair, mock LLM)", () => {
  it("only the genuinely-changed code produces diff hunks", async () => {
    const v1Code = readFixture("v1.0.0");
    const v2Code = readFixture("v1.1.0");

    const renameV1 = createRenamePlugin({ provider: suffixProvider("Named") });
    const resultV1 = await renameV1(v1Code);
    assert.strictEqual(resultV1.parseFailure, undefined);

    const renameV2 = createRenamePlugin({
      provider: suffixProvider("Fresh"),
      priorVersionCode: resultV1.code
    });
    const resultV2 = await renameV2(v2Code);
    assert.strictEqual(resultV2.parseFailure, undefined);

    const diff = diffLines(
      resultV1.code.split("\n"),
      resultV2.code.split("\n")
    );

    // Rename noise: replaced lines identical after identifier masking —
    // the logic did not change, only the names did.
    const renameNoise = diff.replacements.filter(
      ([before, after]) =>
        before !== after && maskIdentifiers(before) === maskIdentifiers(after)
    );
    assert.deepStrictEqual(
      renameNoise,
      [],
      `unchanged logic must keep identical names; noisy line pairs:\n${renameNoise
        .map(([x, y]) => `  - ${x.trim()}\n  + ${y.trim()}`)
        .join("\n")}`
    );

    // The single genuine change is one inserted console.log statement —
    // the added side must contain it, and the diff must stay confined to
    // that neighborhood instead of sprawling across the file.
    assert.ok(
      diff.addedOnly.some((line) => line.includes('"perturbation"')),
      "the genuine insertion must appear in the diff"
    );
    const totalChanged =
      diff.replacements.length +
      diff.addedOnly.length +
      diff.removedOnly.length;
    assert.ok(
      totalChanged <= 6,
      `diff must stay confined to the genuine change, got ${totalChanged} changed lines:\n` +
        [
          ...diff.replacements.map(
            ([x, y]) => `  - ${x.trim()}\n  + ${y.trim()}`
          ),
          ...diff.removedOnly.map((x) => `  - ${x.trim()}`),
          ...diff.addedOnly.map((x) => `  + ${x.trim()}`)
        ].join("\n")
    );
  });
});
