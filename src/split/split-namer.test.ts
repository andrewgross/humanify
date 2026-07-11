import assert from "node:assert";
import { describe, it } from "node:test";
import type { BatchRenameRequest, LLMProvider } from "../llm/types.js";
import type { SplitNameRequest } from "./stable-split.js";
import { createSplitNamer } from "./split-namer.js";

function providerReturning(
  fn: (req: BatchRenameRequest) => Record<string, string>
): LLMProvider {
  return {
    async suggestAllNames(req: BatchRenameRequest) {
      return { renames: fn(req) };
    }
  };
}

const FILE_REQ: SplitNameRequest = {
  kind: "file",
  mechanicalStem: "handleMessageVal",
  siblings: ["createTeammateTag"],
  bindings: ["function handleMessage (12 refs)", "var messageQueue (3 refs)"]
};

const FOLDER_REQ: SplitNameRequest = {
  kind: "folder",
  mechanicalStem: "rgbString",
  siblings: ["persistToDisk"],
  bindings: ["function rgbString (8 refs)", "function hslToRgb (5 refs)"],
  members: ["rgbString", "hslToRgb", "parseColor"]
};

describe("createSplitNamer", () => {
  it("returns the proposed name keyed by the mechanical stem", async () => {
    const namer = createSplitNamer(
      providerReturning((req) => {
        // The stem is the single identifier we ask about.
        assert.deepStrictEqual(req.identifiers, ["handleMessageVal"]);
        // The prompt must carry the bindings and siblings as context.
        assert.match(req.code, /handleMessage/);
        assert.match(req.code, /createTeammateTag/);
        return { handleMessageVal: "handleTeammateMessage" };
      })
    );
    assert.strictEqual(await namer(FILE_REQ), "handleTeammateMessage");
  });

  it("passes the folder's member files in the prompt", async () => {
    let seen = "";
    const namer = createSplitNamer(
      providerReturning((req) => {
        seen = req.code;
        return { rgbString: "colorConversion" };
      })
    );
    assert.strictEqual(await namer(FOLDER_REQ), "colorConversion");
    assert.match(seen, /hslToRgb/);
    assert.match(seen, /parseColor/);
  });

  it("returns null when the model declines or echoes the stem", async () => {
    const decline = createSplitNamer(providerReturning(() => ({})));
    assert.strictEqual(await decline(FILE_REQ), null);
    const echo = createSplitNamer(
      providerReturning(() => ({ handleMessageVal: "handleMessageVal" }))
    );
    assert.strictEqual(await echo(FILE_REQ), null);
  });

  it("contains a provider throw as a null (naming is best-effort)", async () => {
    const crashing: LLMProvider = {
      async suggestAllNames() {
        throw new Error("box down");
      }
    };
    const namer = createSplitNamer(crashing);
    assert.strictEqual(await namer(FILE_REQ), null);
  });
});
