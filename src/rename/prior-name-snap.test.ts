import assert from "node:assert";
import { describe, it } from "node:test";
import {
  buildPriorStemIndex,
  nameStem,
  snapSuggestionToPrior
} from "./prior-name-snap.js";

describe("nameStem", () => {
  it("strips trailing decorations and lowercases", () => {
    assert.strictEqual(nameStem("identityVal"), "identity");
    assert.strictEqual(nameStem("identityVar"), "identity");
    assert.strictEqual(nameStem("appStateValue"), "appstate");
    assert.strictEqual(
      nameStem("normalizedSchemaInstance"),
      "normalizedschema"
    );
    assert.strictEqual(nameStem("React95"), "react");
    assert.strictEqual(nameStem("RpcRequestSchema"), "rpcrequestschema");
    assert.strictEqual(nameStem("rpcRequestSchema"), "rpcrequestschema");
    assert.strictEqual(nameStem("config"), "config");
  });

  it("returns empty for all-decoration names", () => {
    assert.strictEqual(nameStem("Val"), "");
  });
});

describe("snapSuggestionToPrior", () => {
  it("snaps a re-decorated suggestion to the unique same-stem prior name", () => {
    const index = buildPriorStemIndex(["identityVal", "config", "first"]);
    assert.strictEqual(
      snapSuggestionToPrior("identityVar", index),
      "identityVal"
    );
    assert.strictEqual(snapSuggestionToPrior("configVar", index), "config");
    assert.strictEqual(snapSuggestionToPrior("firstValue", index), "first");
    // Case-only flips snap too.
    const schemas = buildPriorStemIndex(["rpcRequestSchema"]);
    assert.strictEqual(
      snapSuggestionToPrior("RpcRequestSchema", schemas),
      "rpcRequestSchema"
    );
  });

  it("never snaps on an ambiguous stem", () => {
    const index = buildPriorStemIndex(["React95", "React103", "ink8"]);
    assert.strictEqual(snapSuggestionToPrior("React99", index), "React99");
  });

  it("passes exact prior names and unrelated suggestions through", () => {
    const index = buildPriorStemIndex(["identityVal"]);
    assert.strictEqual(
      snapSuggestionToPrior("identityVal", index),
      "identityVal"
    );
    assert.strictEqual(
      snapSuggestionToPrior("whollyOther", index),
      "whollyOther"
    );
  });
});
