import assert from "node:assert";
import { describe, it } from "node:test";
import {
  BATCH_RENAME_SYSTEM_PROMPT,
  buildBatchRenamePrompt,
  buildBatchRenameRetryPrompt,
  buildModuleLevelRenameBody,
  buildModuleLevelRetryPrefix,
  MODULE_LEVEL_RENAME_SYSTEM_PROMPT
} from "./prompts.js";

describe("all system prompts warn about global built-ins", () => {
  it("BATCH_RENAME_SYSTEM_PROMPT warns about globals", () => {
    assert.ok(
      BATCH_RENAME_SYSTEM_PROMPT.includes("global"),
      "Should warn about global built-ins"
    );
  });

  it("MODULE_LEVEL_RENAME_SYSTEM_PROMPT warns about globals", () => {
    assert.ok(
      MODULE_LEVEL_RENAME_SYSTEM_PROMPT.includes("global"),
      "Should warn about global built-ins"
    );
  });
});

describe("buildBatchRenameRetryPrompt", () => {
  it("renders specific rejection reasons for duplicates", () => {
    const prompt = buildBatchRenameRetryPrompt(
      "function f(x, y) { return x + y; }",
      ["x", "y"],
      new Set(["config"]),
      { x: "config", y: "total" },
      { duplicates: ["x"], invalid: [], missing: [], unchanged: [] }
    );

    assert.ok(
      prompt.includes('"x" was suggested as "config"'),
      "Should show what was tried"
    );
    assert.ok(prompt.includes("conflicts"), "Should explain the conflict");
  });

  it("renders unchanged identifiers with emphasis", () => {
    const prompt = buildBatchRenameRetryPrompt(
      "function f(z) { return z; }",
      ["z"],
      new Set([]),
      { z: "z" },
      { duplicates: [], invalid: [], missing: [], unchanged: ["z"] }
    );

    assert.ok(
      prompt.includes('"z" was returned as itself'),
      "Should note unchanged"
    );
    assert.ok(
      prompt.includes("MUST suggest a DIFFERENT name"),
      "Should emphasize different"
    );
  });

  it("renders invalid identifiers with the suggested name", () => {
    const prompt = buildBatchRenameRetryPrompt(
      "function f(y) {}",
      ["y"],
      new Set([]),
      { y: "123bad" },
      { duplicates: [], invalid: ["y"], missing: [], unchanged: [] }
    );

    assert.ok(
      prompt.includes('"y" was suggested as "123bad"'),
      "Should show invalid suggestion"
    );
    assert.ok(prompt.includes("not allowed"), "Should explain invalid");
  });

  it("renders global built-in rejection with appropriate message", () => {
    const prompt = buildBatchRenameRetryPrompt(
      "function f(x) { return createTypeChecker('Date'); }",
      ["x"],
      new Set([]),
      { x: "Date" },
      { duplicates: [], invalid: ["x"], missing: [], unchanged: [] }
    );

    assert.ok(
      prompt.includes('"x" was suggested as "Date"'),
      "Should show the rejected global name"
    );
    assert.ok(
      prompt.includes("DO NOT suggest these names"),
      "Should forbid the global name on retry"
    );
    assert.ok(prompt.includes("Date"), "Should list Date in forbidden names");
  });

  it("includes DO NOT suggest list from previous attempt values", () => {
    const prompt = buildBatchRenameRetryPrompt(
      "function f(a, b) {}",
      ["a", "b"],
      new Set(["config"]),
      { a: "config", b: "b" },
      { duplicates: ["a"], invalid: [], missing: [], unchanged: ["b"] }
    );

    assert.ok(
      prompt.includes("DO NOT suggest these names"),
      "Should forbid rejected names"
    );
    assert.ok(prompt.includes("config"), "Should list rejected name");
  });

  it("lists missing identifiers", () => {
    const prompt = buildBatchRenameRetryPrompt(
      "function f(a) {}",
      ["a"],
      new Set([]),
      {},
      { duplicates: [], invalid: [], missing: ["a"], unchanged: [] }
    );

    assert.ok(prompt.includes("MISSING"), "Should mention missing");
    assert.ok(prompt.includes("a"), "Should list missing identifier");
  });
});

describe("buildModuleLevelRenameBody used-names cap", () => {
  it("caps the used-names list so late-run module prompts stay bounded", () => {
    // Module-scope usedNames grows with every rename applied during the
    // run (thousands of descriptive names by the tail of a bundle run).
    // Joining ALL of them overflowed the model context and 400-failed the
    // batch — exp015 baseline: module-binding batches at 45K tokens.
    // Validation still runs against the FULL set; the cap only bounds
    // what the prompt carries.
    const usedNames = new Set<string>();
    for (let i = 0; i < 8000; i++) usedNames.add(`descriptiveName${i}`);
    const isEligible = () => false; // everything is non-eligible → listed

    const body = buildModuleLevelRenameBody(
      ["var ab = 1;"],
      { ab: [] },
      { ab: [] },
      ["ab"],
      usedNames,
      isEligible
    );

    const usedLine = body
      .split("\n")
      .find((l) => l.startsWith("Names already in use"));
    assert.ok(usedLine, "used-names line should be present");
    const listed = usedLine.split(":")[1].split(",").length;
    assert.ok(
      listed <= 200,
      `used-names list must be capped, got ${listed} names`
    );
  });
});

describe("buildModuleLevelRetryPrefix", () => {
  it("renders duplicate rejection with suggested name", () => {
    const prefix = buildModuleLevelRetryPrefix(
      { x: "config" },
      { duplicates: ["x"], invalid: [], missing: [], unchanged: [] }
    );

    assert.ok(
      prefix.includes('"x" was suggested as "config"'),
      "Should show tried name"
    );
    assert.ok(prefix.includes("conflicts"), "Should explain conflict");
  });

  it("renders unchanged identifiers", () => {
    const prefix = buildModuleLevelRetryPrefix(
      { z: "z" },
      { duplicates: [], invalid: [], missing: [], unchanged: ["z"] }
    );

    assert.ok(
      prefix.includes('"z" was returned as itself'),
      "Should note unchanged"
    );
  });

  it("renders invalid identifiers with the suggested name", () => {
    const prefix = buildModuleLevelRetryPrefix(
      { y: "delete" },
      { duplicates: [], invalid: ["y"], missing: [], unchanged: [] }
    );

    assert.ok(
      prefix.includes('"y" was suggested as "delete"'),
      "Should show invalid suggestion"
    );
    assert.ok(prefix.includes("not allowed"), "Should explain invalid");
  });

  it("includes DO NOT suggest list", () => {
    const prefix = buildModuleLevelRetryPrefix(
      { a: "badName" },
      { duplicates: ["a"], invalid: [], missing: [], unchanged: [] }
    );

    assert.ok(
      prefix.includes("DO NOT suggest these names"),
      "Should forbid names"
    );
    assert.ok(prefix.includes("badName"), "Should list rejected name");
  });
});

describe("buildBatchRenameRetryPrompt alreadyRenamed context", () => {
  it("includes already-renamed identifiers when provided", () => {
    const prompt = buildBatchRenameRetryPrompt(
      "function f(a, b, c, d) { return a + b + c + d; }",
      ["c", "d"],
      new Set(["parentDom", "newChildren"]),
      { c: "c", d: "d" },
      { duplicates: [], invalid: [], missing: [], unchanged: ["c", "d"] },
      undefined,
      { a: "parentDom", b: "newChildren" }
    );

    assert.ok(
      prompt.includes("already renamed"),
      "Should mention already renamed"
    );
    assert.ok(
      prompt.includes("a → parentDom"),
      "Should include first renamed pair"
    );
    assert.ok(
      prompt.includes("b → newChildren"),
      "Should include second renamed pair"
    );
  });

  it("does not include already-renamed section when not provided", () => {
    const prompt = buildBatchRenameRetryPrompt(
      "function f(a, b) { return a + b; }",
      ["a", "b"],
      new Set(),
      { a: "a", b: "b" },
      { duplicates: [], invalid: [], missing: [], unchanged: ["a", "b"] }
    );

    assert.ok(
      !prompt.includes("already renamed"),
      "Should not mention already renamed when not provided"
    );
  });
});

describe("buildBatchRenamePrompt prior-version context", () => {
  it("includes prior-version section when context provided", () => {
    const priorCode =
      "function handleError(error, currentFiber) { return error; }";
    const prompt = buildBatchRenamePrompt(
      "function a(b, c) { return b; }",
      ["a", "b", "c"],
      new Set(["existing"]),
      [],
      [],
      undefined,
      priorCode
    );

    assert.ok(prompt.includes("prior version"), "Should mention prior version");
    assert.ok(
      prompt.includes("handleError"),
      "Should include prior function name"
    );
    assert.ok(
      prompt.includes("currentFiber"),
      "Should include prior parameter name"
    );
    assert.ok(
      prompt.includes("MUST reuse"),
      "Should strongly require reusing names"
    );
    assert.ok(prompt.includes("conflict"), "Should mention conflict handling");
  });

  it("omits prior-version section when no context", () => {
    const prompt = buildBatchRenamePrompt(
      "function a(b) { return b; }",
      ["a", "b"],
      new Set(),
      [],
      []
    );

    assert.ok(
      !prompt.includes("prior version"),
      "Should not mention prior version"
    );
  });

  it("lists the prior names explicitly for mechanical reuse", () => {
    const prompt = buildBatchRenamePrompt(
      "function a(b, c) { return b; }",
      ["a", "b", "c"],
      new Set(),
      [],
      [],
      undefined,
      "function handleError(error) { return error; }",
      ["handleError", "error", "currentFiber"]
    );

    assert.ok(
      prompt.includes("handleError, error, currentFiber"),
      `prompt should list prior names verbatim, got:\n${prompt}`
    );
    assert.ok(
      /reuse these names/i.test(prompt),
      "prompt should instruct reuse of the listed names"
    );
  });

  it("renders already-transferred names on the first round", () => {
    const prompt = buildBatchRenamePrompt(
      "function a(b) { const userId = b.id; return userId; }",
      ["a", "b"],
      new Set(),
      [],
      [],
      undefined,
      "function getUser(request) { const userId = request.id; return userId; }",
      undefined,
      { t: "userId" }
    );

    assert.ok(
      prompt.includes("t → userId"),
      `prompt should render transferred pairs, got:\n${prompt}`
    );
    assert.ok(
      /already renamed/i.test(prompt),
      "prompt should say these were already renamed"
    );
  });
});
