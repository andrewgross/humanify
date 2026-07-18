/**
 * Functional validation: adversarial rename suggestions must never change
 * runtime behavior through shadowing or capture.
 *
 * The 2.1.166 walk output shipped a capture — an inner env local renamed
 * to the outer transport variable's name swallowed the outer's assignment
 * — that parsed cleanly and passed every count-based gate. These tests
 * run the REAL pipeline (createRenamePlugin) against providers that
 * actively suggest capturing names, then EXECUTE original and humanified
 * modules and compare results.
 *
 * A canary test applies a capture by hand and asserts the behavioral
 * harness detects it — proving the parity assertions have teeth.
 */
import assert from "node:assert";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { describe, it } from "node:test";
import type { BatchRenameRequest, LLMProvider } from "../llm/types.js";
import { createRenamePlugin } from "./plugin.js";

/**
 * Minified-shaped fixture with the scoping traps that make captures
 * possible:
 * - connectTransport: outer `q` assigned INSIDE branch blocks that declare
 *   locals (`w`, `u`) and read after them — renaming a local to q's new
 *   name captures the assignment (the 2.1.166 bug shape).
 * - accumulate: loop-scoped closure captures (`v` per iteration).
 * - bumpTwice: outer written inside a block that declares an inner local.
 */
const FIXTURE = `
function connectTransport(g) {
  let q;
  if (g.kind === "stdio") {
    let w = { base: g.base };
    q = { kind: "stdio", env: { ...w, extra: 1 } };
  } else if (g.kind === "ws") {
    let u = g.url + "/ws";
    q = { kind: "ws", url: u };
  }
  let r = q;
  return r ? { ...r, ok: true } : { ok: false };
}
function accumulate(l) {
  let t = 0;
  const a = [];
  for (let i = 0; i < l.length; i++) {
    const v = l[i];
    a.push(() => v * 2);
  }
  for (const f of a) t += f();
  return t;
}
function bumpTwice(x) {
  let o = x + 1;
  {
    let n = o + 1;
    o = n + 1;
  }
  return o;
}
export { connectTransport, accumulate, bumpTwice };
`;

/** Write code to a temp file and dynamically import it. */
async function importCode(code: string): Promise<Record<string, unknown>> {
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "humanify-capture-"));
  const tmpFile = path.join(
    tmpDir,
    `bundle-${Math.random().toString(36).slice(2)}.mjs`
  );
  fs.writeFileSync(tmpFile, code);
  try {
    return await import(tmpFile);
  } finally {
    fs.unlinkSync(tmpFile);
    fs.rmdirSync(tmpDir);
  }
}

interface FixtureExports {
  connectTransport: (g: Record<string, unknown>) => unknown;
  accumulate: (l: number[]) => number;
  bumpTwice: (x: number) => number;
}

/** Run every exported behavior on fixed inputs. */
function behaviorOf(mod: FixtureExports) {
  return {
    stdio: mod.connectTransport({ kind: "stdio", base: 2 }),
    ws: mod.connectTransport({ kind: "ws", url: "srv" }),
    none: mod.connectTransport({ kind: "none" }),
    sum: mod.accumulate([1, 2, 3]),
    bump: mod.bumpTwice(5)
  };
}

async function assertBehaviorPreserved(provider: LLMProvider, label: string) {
  const plugin = createRenamePlugin({ provider, concurrency: 2 });
  const result = await plugin(FIXTURE);
  assert.strictEqual(
    result.parseFailure,
    undefined,
    `${label}: output must parse`
  );
  assert.strictEqual(
    result.semanticFailure,
    undefined,
    `${label}: rename invariants must hold`
  );
  const original = behaviorOf(
    (await importCode(FIXTURE)) as unknown as FixtureExports
  );
  const humanified = behaviorOf(
    (await importCode(result.code)) as unknown as FixtureExports
  );
  assert.deepStrictEqual(
    humanified,
    original,
    `${label}: humanified module must behave identically\n--- output ---\n${result.code}`
  );
}

describe("adversarial rename functional validation", () => {
  it("capture-bait provider (outer's name suggested for inner locals) preserves behavior", async () => {
    // The exact 2.1.166 shape: the inner env local and the outer transport
    // variable are both suggested the SAME name; same for the bump pair.
    const bait: Record<string, string> = {
      q: "transportInstance",
      w: "transportInstance",
      u: "transportInstance",
      r: "transportResult",
      o: "bumpCounter",
      n: "bumpCounter",
      t: "totalSum",
      v: "totalSum",
      i: "totalSum",
      a: "adderList",
      f: "adderFn",
      g: "serverConfig",
      l: "itemList",
      x: "startValue"
    };
    const provider: LLMProvider = {
      async suggestAllNames(request: BatchRenameRequest) {
        const renames: Record<string, string> = {};
        for (const id of request.identifiers) {
          renames[id] = bait[id] ?? `humanified_${id}`;
        }
        return { renames };
      }
    };
    await assertBehaviorPreserved(provider, "capture-bait");
  });

  it("one-name-for-everything provider preserves behavior", async () => {
    const provider: LLMProvider = {
      async suggestAllNames(request: BatchRenameRequest) {
        const renames: Record<string, string> = {};
        for (const id of request.identifiers) renames[id] = "sharedState";
        return { renames };
      }
    };
    await assertBehaviorPreserved(provider, "one-name");
  });

  it("canary: the harness detects a hand-made capture", async () => {
    // Manually introduce the 2.1.166 bug: the inner env local takes the
    // outer variable's name, so `transportInstance = { kind: "stdio", ... }`
    // assigns the INNER binding and the outer stays undefined.
    const broken = FIXTURE.replace(/let q;/, "let transportInstance;")
      .replace(
        /let w = \{ base: g.base \};/,
        "let transportInstance = { base: g.base };"
      )
      .replace(
        /q = \{ kind: "stdio", env: \{ \.\.\.w, extra: 1 \} \};/,
        'transportInstance = { kind: "stdio", env: { ...transportInstance, extra: 1 } };'
      )
      .replace(
        /q = \{ kind: "ws", url: u \};/,
        'transportInstance = { kind: "ws", url: u };'
      )
      .replace(/let r = q;/, "let r = transportInstance;");
    const original = behaviorOf(
      (await importCode(FIXTURE)) as unknown as FixtureExports
    );
    const captured = behaviorOf(
      (await importCode(broken)) as unknown as FixtureExports
    );
    assert.notDeepStrictEqual(
      captured,
      original,
      "the capture must change observable behavior (stdio result lost)"
    );
    assert.deepStrictEqual(
      captured.stdio,
      { ok: false },
      "captured stdio branch loses the outer assignment"
    );
  });
});
