import assert from "node:assert";
import test from "node:test";
import { humanify } from "./test-utils.js";

test("throws error on missing file", async () => {
  await assert.rejects(humanify("nonexistent-file.js"));
});

test("throws error on missing file (local mode)", async () => {
  await assert.rejects(humanify("--local", "nonexistent-file.js"));
});

test("local throws error on missing model", async () => {
  await assert.rejects(
    humanify("--local", "--local-model", "nonexistent-model", "dummy.js")
  );
});
