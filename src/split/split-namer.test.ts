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
  it("returns proposed names keyed by each mechanical stem", async () => {
    const namer = createSplitNamer(
      providerReturning((req) => {
        // The stems are the identifiers we ask about, in request order.
        assert.deepStrictEqual(req.identifiers, ["handleMessageVal"]);
        // The prompt must carry the bindings and siblings as context.
        assert.match(req.code, /handleMessage/);
        assert.match(req.code, /createTeammateTag/);
        return { handleMessageVal: "handleTeammateMessage" };
      })
    );
    assert.deepStrictEqual(await namer([FILE_REQ]), ["handleTeammateMessage"]);
  });

  it("names a whole batch in ONE provider call", async () => {
    let calls = 0;
    const namer = createSplitNamer(
      providerReturning((req) => {
        calls++;
        assert.deepStrictEqual(req.identifiers, [
          "handleMessageVal",
          "rgbString"
        ]);
        return {
          handleMessageVal: "handleTeammateMessage",
          rgbString: "colorConversion"
        };
      })
    );
    assert.deepStrictEqual(await namer([FILE_REQ, FOLDER_REQ]), [
      "handleTeammateMessage",
      "colorConversion"
    ]);
    assert.strictEqual(calls, 1, "one batch = one provider call");
  });

  it("uniquifies duplicate stems within a batch and maps results back", async () => {
    const twin: SplitNameRequest = { ...FILE_REQ, siblings: [] };
    const namer = createSplitNamer(
      providerReturning((req) => {
        // Two requests share a stem; the keys sent to the model must be
        // unique so each brief maps to exactly one answer.
        assert.strictEqual(new Set(req.identifiers).size, 2);
        const [first, second] = req.identifiers;
        return { [first]: "inboundMessages", [second]: "outboundMessages" };
      })
    );
    assert.deepStrictEqual(await namer([FILE_REQ, twin]), [
      "inboundMessages",
      "outboundMessages"
    ]);
  });

  it("passes the folder's member files in the prompt", async () => {
    let seen = "";
    const namer = createSplitNamer(
      providerReturning((req) => {
        seen = req.code;
        return { rgbString: "colorConversion" };
      })
    );
    assert.deepStrictEqual(await namer([FOLDER_REQ]), ["colorConversion"]);
    assert.match(seen, /hslToRgb/);
    assert.match(seen, /parseColor/);
  });

  it("returns null per entry when the model declines or echoes the stem", async () => {
    const decline = createSplitNamer(providerReturning(() => ({})));
    assert.deepStrictEqual(await decline([FILE_REQ]), [null]);
    const echo = createSplitNamer(
      providerReturning(() => ({ handleMessageVal: "handleMessageVal" }))
    );
    assert.deepStrictEqual(await echo([FILE_REQ]), [null]);
  });

  it("contains a provider throw as all-null (naming is best-effort)", async () => {
    const crashing: LLMProvider = {
      async suggestAllNames() {
        throw new Error("box down");
      }
    };
    const namer = createSplitNamer(crashing);
    assert.deepStrictEqual(await namer([FILE_REQ, FOLDER_REQ]), [null, null]);
  });

  it("returns an empty array for an empty batch without calling the provider", async () => {
    const namer = createSplitNamer(
      providerReturning(() => {
        throw new Error("must not be called");
      })
    );
    assert.deepStrictEqual(await namer([]), []);
  });
});
