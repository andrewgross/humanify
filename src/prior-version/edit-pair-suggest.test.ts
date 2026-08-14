import assert from "node:assert";
import { describe, it } from "node:test";
import { parseSync } from "@babel/core";
import type { NodePath } from "@babel/traverse";
import type * as t from "@babel/types";
import { traverse } from "../babel-utils.js";
import { computeEditPairSuggestions } from "./edit-pair-suggest.js";

function statementTexts(code: string): (string | null)[] {
  return statements(code).map((p) => p.toString());
}

function statements(code: string): NodePath[] {
  const ast = parseSync(code, {
    sourceType: "unambiguous",
    configFile: false,
    babelrc: false
  }) as t.File;
  let out: NodePath[] = [];
  traverse(ast, {
    Program(p: NodePath<t.Program>) {
      out = p.get("body") as NodePath[];
      p.stop();
    }
  });
  return out;
}

describe("computeEditPairSuggestions", () => {
  it("suggests the prior name across a hash-flipping edit when gates hold", () => {
    // The exp069 population: the statement genuinely changed (extra call →
    // different hash, no twin), the binding's role did not. The pair aligns
    // by masked head + token overlap; the declaration line is name-only.
    const prior = statementTexts(`
      var loadConfiguration = (configPath) => {
        const raw = readFile(configPath);
        trackUsage(raw);
        return parse(raw);
      };
    `);
    const fresh = statementTexts(`
      var q1 = (q2) => {
        const q3 = readFile(q2);
        trackUsage(q3);
        auditAccess(q2);
        return parse(q3);
      };
    `);
    const suggestions = computeEditPairSuggestions(prior, [
      { text: fresh[0], pendingBindingNames: new Set(["q1"]) }
    ]);
    assert.strictEqual(suggestions.get("q1"), "loadConfiguration");
  });

  it("refuses when the masked head is ambiguous on either side", () => {
    const prior = statementTexts(`
      var alpha = (x) => { one(x); shared(x); };
      var beta = (y) => { two(y); shared(y); };
    `);
    const fresh = statementTexts(`
      var q1 = (z) => { one(z); shared(z); extra(); };
    `);
    // Both prior statements mask to the same head — no unique pairing.
    const suggestions = computeEditPairSuggestions(prior, [
      { text: fresh[0], pendingBindingNames: new Set(["q1"]) }
    ]);
    assert.strictEqual(suggestions.size, 0);
  });

  it("refuses a role change — token overlap below the floor", () => {
    // The getTempDirPath principle: same shape of head, different body.
    const prior = statementTexts(`
      var computeChecksum = (input) => {
        const digest = sha256(input);
        return hex(digest);
      };
    `);
    const fresh = statementTexts(`
      var q1 = (q2) => {
        const q3 = fetchRemote(q2);
        retryWithBackoff(q3);
        return normalize(q3);
      };
    `);
    const suggestions = computeEditPairSuggestions(prior, [
      { text: fresh[0], pendingBindingNames: new Set(["q1"]) }
    ]);
    assert.strictEqual(suggestions.size, 0);
  });

  it("drops non-unanimous identifiers and keeps unanimous ones", () => {
    const prior = statementTexts(`
      var writeReport = (reportPath) => {
        emit(reportPath, writeReport);
        emitLater(otherThing, writeReport);
      };
    `);
    const fresh = statementTexts(`
      var q1 = (q2) => {
        emit(q2, q1);
        emitLater(q2, q1);
      };
    `);
    // q2 pairs with reportPath on line 1 but otherThing on line 2 —
    // non-unanimous, dropped; q1 stays unanimous (writeReport both lines).
    const suggestions = computeEditPairSuggestions(prior, [
      { text: fresh[0], pendingBindingNames: new Set(["q1", "q2"]) }
    ]);
    assert.strictEqual(suggestions.get("q1"), "writeReport");
    assert.strictEqual(suggestions.has("q2"), false);
  });

  it("never suggests a below-floor prior name", () => {
    const prior = statementTexts(`
      var M2_ = (input) => {
        transform(input);
        return finalize(input);
      };
    `);
    const fresh = statementTexts(`
      var q1 = (q2) => {
        transform(q2);
        audit(q2);
        return finalize(q2);
      };
    `);
    const suggestions = computeEditPairSuggestions(prior, [
      { text: fresh[0], pendingBindingNames: new Set(["q1"]) }
    ]);
    assert.strictEqual(suggestions.size, 0);
  });
});
