import assert from "node:assert";
import { describe, it } from "node:test";
import { parseSync } from "@babel/core";
import type { Scope } from "@babel/traverse";
import type * as t from "@babel/types";
import type { BindingRole } from "../prior-version/binding-role.js";
import { carriedNames } from "./carried-names.js";
import { traverse } from "../babel-utils.js";
import {
  rankVoteSuggestion,
  trySingleVotePin,
  type VoteCount
} from "./single-vote-pin.js";

function parseProgramScope(code: string): Scope {
  const ast = parseSync(code, { sourceType: "module" });
  if (!ast) throw new Error("Failed to parse test fixture");
  let programScope: Scope | undefined;
  traverse(ast as t.File, {
    Program(path) {
      programScope = path.scope;
    }
  });
  if (!programScope) throw new Error("No program scope");
  return programScope;
}

function role(overrides: Partial<BindingRole> = {}): BindingRole {
  return {
    structuralHash: "h1",
    contentShingles: null,
    fnCalleeIds: [],
    hasBindingCallees: false,
    ...overrides
  };
}

function votes(entries: [string, VoteCount][]): Map<string, VoteCount> {
  return new Map(entries);
}

/** One exact vote for `packItem`, roles hash-equal — the pin's happy path. */
function baseRequest(scope: Scope) {
  return {
    votes: votes([["packItem", { total: 1, exact: 1 }]]),
    nameClaimants: new Map([["packItem", 1]]),
    priorRoles: new Map([["packItem", role()]]),
    fnMatches: new Map<string, string>(),
    freshRole: () => role(),
    scope,
    oldName: "q7"
  };
}

describe("trySingleVotePin (shared single-vote ladder)", () => {
  it("pins on one exact vote with hash-equal roles and renames the binding", () => {
    const scope = parseProgramScope("function q7(v) { return v; } q7(1);");
    const result = trySingleVotePin(baseRequest(scope));
    assert.ok(result.pinned, `expected pin, got ${JSON.stringify(result)}`);
    assert.strictEqual(result.pinned && result.name, "packItem");
    assert.ok(scope.getBinding("packItem"), "binding renamed in scope");
    assert.ok(!scope.getBinding("q7"), "old name released");
  });

  it("refuses when more than one name has votes", () => {
    const scope = parseProgramScope("function q7(v) { return v; }");
    const result = trySingleVotePin({
      ...baseRequest(scope),
      votes: votes([
        ["packItem", { total: 1, exact: 1 }],
        ["packOther", { total: 1, exact: 1 }]
      ])
    });
    assert.ok(!result.pinned);
    assert.strictEqual(result.pinned || result.blocked, undefined);
    assert.ok(scope.getBinding("q7"), "binding untouched");
  });

  it("refuses a vote without exact slot testimony (demoted twin votes)", () => {
    const scope = parseProgramScope("function q7(v) { return v; }");
    const result = trySingleVotePin({
      ...baseRequest(scope),
      votes: votes([["packItem", { total: 1, exact: 0 }]])
    });
    assert.ok(!result.pinned);
    assert.strictEqual(!result.pinned && result.blocked, "non-exact-source");
  });

  it("refuses when a second binding claims the same prior name", () => {
    const scope = parseProgramScope("function q7(v) { return v; }");
    const result = trySingleVotePin({
      ...baseRequest(scope),
      nameClaimants: new Map([["packItem", 2]])
    });
    assert.ok(!result.pinned);
    assert.strictEqual(!result.pinned && result.blocked, "name-conflict");
  });

  it("refuses when the prior name has no role evidence", () => {
    const scope = parseProgramScope("function q7(v) { return v; }");
    const result = trySingleVotePin({
      ...baseRequest(scope),
      priorRoles: new Map()
    });
    assert.ok(!result.pinned);
    assert.strictEqual(!result.pinned && result.blocked, "no-prior-role");
  });

  it("refuses when roles do not corroborate (different hash, no shingles)", () => {
    const scope = parseProgramScope("function q7(v) { return v; }");
    const result = trySingleVotePin({
      ...baseRequest(scope),
      priorRoles: new Map([["packItem", role({ structuralHash: "h2" })]])
    });
    assert.ok(!result.pinned);
    assert.match((!result.pinned && result.blocked) || "", /^role-mismatch:/);
  });

  it("refuses via the callee-identity veto when mapped callees differ", () => {
    const scope = parseProgramScope("function q7(v) { return v; }");
    const result = trySingleVotePin({
      ...baseRequest(scope),
      priorRoles: new Map([
        ["packItem", role({ fnCalleeIds: ["prior:helperA"] })]
      ]),
      fnMatches: new Map([["prior:helperA", "new:helperA"]]),
      freshRole: () => role({ fnCalleeIds: ["new:helperB"] })
    });
    assert.ok(!result.pinned);
    assert.strictEqual(
      !result.pinned && result.blocked,
      "role-mismatch:callee-mismatch"
    );
  });

  it("refuses when the target name is already held in scope, without retry", () => {
    const scope = parseProgramScope(
      "function q7(v) { return v; } function packItem(x) { return x; }"
    );
    const result = trySingleVotePin(baseRequest(scope));
    assert.ok(!result.pinned);
    assert.match((!result.pinned && result.blocked) || "", /^validation:/);
    assert.ok(scope.getBinding("q7"), "binding untouched on collision");
  });

  it("pins via shingle overlap when hashes are absent", () => {
    const scope = parseProgramScope("function q7(v) { return v; }");
    const shared = new Set(["a", "b", "c", "d"]);
    const result = trySingleVotePin({
      ...baseRequest(scope),
      priorRoles: new Map([
        ["packItem", role({ structuralHash: null, contentShingles: shared })]
      ]),
      freshRole: () =>
        role({
          structuralHash: "different",
          contentShingles: new Set([...shared, "e"])
        })
    });
    assert.ok(
      result.pinned,
      `shingle overlap at the floor should pin, got ${JSON.stringify(result)}`
    );
    assert.strictEqual(result.pinned && result.roleReason, "shingle-overlap");
  });
});

describe("trySingleVotePin — decorated prior names (exp035 E)", () => {
  it("carries a below-floor prior name and registers it (exp066 provenance rule)", () => {
    // The refusal this test used to assert bought a fresh LLM ask every
    // hop (the exp065 fs2/n0e loop). A prior-output name was already
    // processed once: carry it for stability, register it so the sweep
    // will not re-roll it, and let the minted census keep it visible.
    carriedNames.reset(true);
    const scope = parseProgramScope("function q7(v) { return v; }");
    const result = trySingleVotePin({
      ...baseRequest(scope),
      votes: votes([["M2_", { total: 1, exact: 1 }]]),
      nameClaimants: new Map([["M2_", 1]]),
      priorRoles: new Map([["M2_", role()]])
    });
    assert.ok(result.pinned, `expected carry, got ${JSON.stringify(result)}`);
    assert.strictEqual(result.pinned && result.name, "M2_");
    const binding = scope.getBinding("M2_");
    assert.ok(binding, "binding renamed to the carried name");
    assert.ok(
      carriedNames.isCarried(binding),
      "carried below-floor name must be registered for the sweep exemption"
    );
    carriedNames.reset(true);
  });

  it("pins a collision-decorated descriptive prior name", () => {
    // fsPromises_-class names are the LLM's good name wearing a `_` the
    // conflict ladder appended (exp035 task B). Refusing them re-rolls
    // the binding at the LLM every hop — the guard's draw-dependent
    // idempotence channel. A decorated name is a name: pin it.
    const scope = parseProgramScope("function q7(v) { return v; }");
    const result = trySingleVotePin({
      ...baseRequest(scope),
      votes: votes([["initializeApp_", { total: 1, exact: 1 }]]),
      nameClaimants: new Map([["initializeApp_", 1]]),
      priorRoles: new Map([["initializeApp_", role()]])
    });
    assert.ok(result.pinned, `expected pin, got ${JSON.stringify(result)}`);
  });
});

describe("rankVoteSuggestion", () => {
  const v = (entries: [string, VoteCount][]) => new Map(entries);

  it("picks the unique top name, exact votes ranked above totals", () => {
    // exp061: two exact slot testimonies beat three close-sourced votes —
    // exact matches carry byte-level statement identity, close ones do not.
    const suggestion = rankVoteSuggestion(
      v([
        ["writeConfig", { total: 2, exact: 2 }],
        ["persistSettings", { total: 3, exact: 0 }]
      ])
    );
    assert.strictEqual(suggestion, "writeConfig");
  });

  it("abstains on a tie — ambiguous testimony must not bias the ask", () => {
    const suggestion = rankVoteSuggestion(
      v([
        ["writeConfig", { total: 1, exact: 1 }],
        ["persistSettings", { total: 1, exact: 1 }]
      ])
    );
    assert.strictEqual(suggestion, null);
  });

  it("excludes below-floor names from candidacy entirely", () => {
    // A minted leftover (M2_) is a naming gap, not a name (the __m
    // poisoning class): it must neither win nor block the descriptive
    // runner-up. Decorated descriptives (initializeApp152) stay eligible,
    // matching the pin ladder's precedent.
    const suggestion = rankVoteSuggestion(
      v([
        ["M2_", { total: 3, exact: 3 }],
        ["writeConfig", { total: 1, exact: 1 }]
      ])
    );
    assert.strictEqual(suggestion, "writeConfig");
  });

  it("returns null when every candidate is below floor", () => {
    const suggestion = rankVoteSuggestion(v([["M2_", { total: 2, exact: 2 }]]));
    assert.strictEqual(suggestion, null);
  });

  it("breaks an exact-count tie by total votes", () => {
    const suggestion = rankVoteSuggestion(
      v([
        ["writeConfig", { total: 3, exact: 1 }],
        ["persistSettings", { total: 1, exact: 1 }]
      ])
    );
    assert.strictEqual(suggestion, "writeConfig");
  });
});
