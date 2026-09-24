import assert from "node:assert";
import { after, describe, it } from "node:test";
import * as fs from "node:fs";
import * as path from "node:path";
import * as os from "node:os";
import { artifactDump } from "./artifacts.js";
import { writeDumpArtifacts } from "./write.js";
import type { Binding } from "@babel/traverse";
import { strategyTrail } from "../rename/strategy-trail.js";

/**
 * Regression test for the anchor-registration drop: the flatten refactor
 * removed the generated/reconciled anchor registrations, so rows anchored
 * to those texts passed through with RAW UTF-16 spans (unconverted, silent).
 * The conversion is only observable on a text where UTF-16 index != byte
 * offset, so the fixture texts carry multi-byte characters and the asserted
 * byte offsets differ from the raw ones.
 */
describe("writeDumpArtifacts anchor registration", () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "dump-anchors-"));
  after(() => {
    fs.rmSync(dir, { recursive: true, force: true });
  });

  it("converts rows anchored to every registered text", () => {
    // "x = 'é中';" — x0 ' '1 =2 ' '3 '4 é5 中6 '7 ;8. é = 2 bytes (5-6),
    // 中 = 3 bytes (7-9). A raw span [6,7) (中) converts to [7,9).
    const fresh = "x = 'é中';";
    const generated = "x = 'é中';";
    artifactDump.reset(true, { model: "m", temperature: 0 });
    artifactDump.texts.fresh = fresh;
    artifactDump.texts.generated = generated;

    artifactDump.functions = [
      {
        key: { text: "fresh", start: 6, end: 7 },
        sessionId: "t:1:0",
        kind: "function",
        name: "中",
        nameBinding: null,
        structuralHash: "aaaa",
        internalCallees: [],
        scopeParent: null,
        bindings: []
      }
    ];
    // A prompt dispatched with its target anchored to the GENERATED text.
    artifactDump.recordPrompt(
      {
        code: "c",
        identifiers: ["a"],
        usedNames: new Set(),
        calleeSignatures: [],
        callsites: []
      },
      {
        functionId: "t:1:0",
        site: "naming",
        targets: [{ sessionId: "t:1:0", start: 6, end: 7 }],
        targetsText: "generated"
      }
    );

    writeDumpArtifacts({ dir, flags: {}, outputDir: dir });

    const fns = JSON.parse(
      fs.readFileSync(path.join(dir, "functions.json"), "utf8")
    );
    assert.strictEqual(fns.functions[0].key.start, 7, "fresh row converted");

    const prompt = JSON.parse(
      fs
        .readFileSync(path.join(dir, "prompts.jsonl"), "utf8")
        .split("\n")
        .filter(Boolean)[0]
    );
    assert.strictEqual(prompt.targets[0].text, "generated");
    assert.strictEqual(
      prompt.targets[0].start,
      7,
      "generated-anchored row converted — an unregistered anchor would pass 6 through raw"
    );
  });
});

/**
 * The mechanical-stage boundary snapshot (phase 3's gate): the strategy
 * trail frozen at the moment the prior-version transfer stage has finished
 * and the LLM waves have not started. transfers.json is the FINAL trail —
 * it cannot gate the mechanical tiers alone.
 */
describe("transfers-mechanical.json (mechanical-stage boundary)", () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "dump-mechanical-"));
  after(() => {
    fs.rmSync(dir, { recursive: true, force: true });
    strategyTrail.reset(false);
    artifactDump.reset(false);
  });

  /** A Binding stand-in: the trail reads only the declaration identifier. */
  function fakeBinding(start: number, end: number): Binding {
    return {
      identifier: {
        start,
        end,
        loc: { start: { line: 1, column: start } }
      }
    } as unknown as Binding;
  }

  it("is inert when the dump is not armed", () => {
    strategyTrail.reset(true);
    artifactDump.reset(false);
    strategyTrail.record(fakeBinding(0, 1), "a", {
      strategy: "exact-match",
      outcome: "applied",
      newName: "alpha"
    });
    artifactDump.captureMechanicalBoundary();
    assert.strictEqual(artifactDump.mechanicalTrails, null);
  });

  it("freezes the trail at the boundary — later LLM attempts do not leak in", () => {
    strategyTrail.reset(true);
    artifactDump.reset(true, { model: "m", temperature: 0 });
    artifactDump.texts.fresh = "a;b;c;";
    const a = fakeBinding(0, 1);
    const b = fakeBinding(2, 3);
    strategyTrail.record(a, "a", {
      strategy: "exact-match",
      outcome: "applied",
      newName: "alpha"
    });
    strategyTrail.record(b, "b", {
      strategy: "close-match",
      outcome: "rejected",
      reason: "collision",
      newName: "beta"
    });

    artifactDump.captureMechanicalBoundary();

    // The waves: an LLM rename of b, and a brand-new binding c.
    strategyTrail.record(b, "b", {
      strategy: "llm",
      outcome: "applied",
      newName: "bravo"
    });
    strategyTrail.record(fakeBinding(4, 5), "c", {
      strategy: "llm",
      outcome: "applied",
      newName: "charlie"
    });

    writeDumpArtifacts({ dir, flags: {}, outputDir: dir });

    const mech = JSON.parse(
      fs.readFileSync(path.join(dir, "transfers-mechanical.json"), "utf8")
    );
    assert.deepStrictEqual(mech.transfers, [
      {
        target: { text: "fresh", start: 0, end: 1 },
        oldName: "a",
        finalName: "alpha",
        settledBy: "exact-match",
        attempts: [
          { tier: "exact-match", outcome: "applied", proposedName: "alpha" }
        ]
      },
      {
        target: { text: "fresh", start: 2, end: 3 },
        oldName: "b",
        finalName: null,
        attempts: [
          {
            tier: "close-match",
            outcome: "rejected",
            reason: "collision",
            proposedName: "beta"
          }
        ]
      }
    ]);

    // The final trail still carries the waves' work.
    const final = JSON.parse(
      fs.readFileSync(path.join(dir, "transfers.json"), "utf8")
    );
    assert.strictEqual(final.transfers.length, 3);
    assert.strictEqual(final.transfers[1].settledBy, "llm");
  });

  it("writes no file when the boundary was never reached", () => {
    const empty = fs.mkdtempSync(path.join(os.tmpdir(), "dump-mech-none-"));
    try {
      strategyTrail.reset(true);
      artifactDump.reset(true, { model: "m", temperature: 0 });
      artifactDump.texts.fresh = "a;";
      writeDumpArtifacts({ dir: empty, flags: {}, outputDir: empty });
      assert.strictEqual(
        fs.existsSync(path.join(empty, "transfers-mechanical.json")),
        false
      );
    } finally {
      fs.rmSync(empty, { recursive: true, force: true });
    }
  });
});
