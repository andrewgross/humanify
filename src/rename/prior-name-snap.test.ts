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

describe("snapSuggestionToPrior exact-slot synonym snap (A2)", () => {
  it("snaps a full synonym flip back to the per-slot prior name", () => {
    // Different stem from every prior name — the stem index cannot catch it.
    const index = buildPriorStemIndex(["caughtError"]);
    const snaps = { x: "caughtError" };
    assert.strictEqual(
      snapSuggestionToPrior("decisionOutcome", index, "x", snaps),
      "caughtError"
    );
  });

  it("only snaps the exact slot that carries snap evidence", () => {
    const index = buildPriorStemIndex(["caughtError"]);
    const snaps = { x: "caughtError" };
    // A different identifier y has no per-slot snap → left to the LLM.
    assert.strictEqual(
      snapSuggestionToPrior("decisionOutcome", index, "y", snaps),
      "decisionOutcome"
    );
  });

  it("is a no-op when the LLM already chose the slot's prior name", () => {
    const index = new Map<string, string>();
    assert.strictEqual(
      snapSuggestionToPrior("caughtError", index, "x", { x: "caughtError" }),
      "caughtError"
    );
  });

  it("keeps the same-stem behavior when no per-slot snap applies", () => {
    const index = buildPriorStemIndex(["identityVal"]);
    // No snap map for this slot → falls back to the stem index.
    assert.strictEqual(
      snapSuggestionToPrior("identityVar", index, "z", { x: "caughtError" }),
      "identityVal"
    );
  });
});

describe("decoration-churn cases the stem snap already collapses (A3)", () => {
  // Regression guards for the mechanical decoration bugs the noise-levers plan
  // (A3) called out. Investigation found NO Val/Text append site nor a local
  // ordinal counter in the rename layer — these are LLM output, and the stem
  // snap already folds them back to the prior name whenever prior context is
  // present. A1's per-id hints + A2's snap extend that to the exact slot
  // (beyond the 40-name flat bag and to full synonym flips). These tests lock
  // the collapse in so a future refactor cannot silently regress it.

  it("collapses a doubled decoration suffix back to the prior name (upstreamConfigValVal → upstreamConfigVal)", () => {
    const index = buildPriorStemIndex(["upstreamConfigVal"]);
    assert.strictEqual(
      snapSuggestionToPrior("upstreamConfigValVal", index),
      "upstreamConfigVal"
    );
  });

  it("snaps an ordinal reshuffle back to the prior ordinal (react23 → React219)", () => {
    const index = buildPriorStemIndex(["React219"]);
    assert.strictEqual(snapSuggestionToPrior("react23", index), "React219");
  });

  it("does NOT treat a meaningful trailing word as a decoration (Text/Error are not stems)", () => {
    // `errorMessage` → `errorMessageText` cannot be stem-collapsed, and MUST
    // not be: `Text`/`Error` carry meaning, so stem-folding them would let
    // `handleText` mis-snap onto `handleError`. Suffix-drift is safely fixed
    // only by A1/A2's per-slot identity, never by widening the stem vocabulary.
    assert.notStrictEqual(
      nameStem("errorMessage"),
      nameStem("errorMessageText")
    );
    assert.notStrictEqual(nameStem("handleError"), nameStem("handleText"));
    const index = buildPriorStemIndex(["errorMessage"]);
    assert.strictEqual(
      snapSuggestionToPrior("errorMessageText", index),
      "errorMessageText",
      "a meaningful-suffix drift must pass through the stem snap untouched"
    );
  });
});
