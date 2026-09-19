import assert from "node:assert";
import { after, describe, it } from "node:test";
import * as fs from "node:fs";
import * as path from "node:path";
import * as os from "node:os";
import { artifactDump } from "./artifacts.js";
import { writeDumpArtifacts } from "./write.js";

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
