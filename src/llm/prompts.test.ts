import { describe, it } from "node:test";
import assert from "node:assert";
import {
  SYSTEM_PROMPT,
  FUNCTION_NAME_SYSTEM_PROMPT,
  buildUserPrompt,
  buildFunctionNamePrompt,
  buildRetryPrompt,
  buildFunctionRetryPrompt,
  IDENTIFIER_GRAMMAR,
  JSON_NAME_GRAMMAR,
  JSON_NAME_REASONING_GRAMMAR
} from "./prompts.js";
import type { LLMContext } from "../analysis/types.js";

describe("SYSTEM_PROMPT", () => {
  it("includes naming guidelines", () => {
    assert.ok(SYSTEM_PROMPT.includes("camelCase"), "Should mention camelCase");
    assert.ok(SYSTEM_PROMPT.includes("PascalCase"), "Should mention PascalCase");
  });

  it("mentions JSON response format", () => {
    assert.ok(SYSTEM_PROMPT.includes("JSON"), "Should mention JSON");
    assert.ok(SYSTEM_PROMPT.includes("name"), "Should mention name field");
    assert.ok(SYSTEM_PROMPT.includes("reasoning"), "Should mention reasoning");
  });

  it("warns about reserved words", () => {
    assert.ok(SYSTEM_PROMPT.includes("reserved"), "Should mention reserved words");
  });
});

describe("FUNCTION_NAME_SYSTEM_PROMPT", () => {
  it("includes function-specific guidelines", () => {
    assert.ok(
      FUNCTION_NAME_SYSTEM_PROMPT.includes("verb"),
      "Should mention starting with verb"
    );
    assert.ok(
      FUNCTION_NAME_SYSTEM_PROMPT.includes("function"),
      "Should mention functions"
    );
  });

  it("mentions constructors and classes", () => {
    assert.ok(
      FUNCTION_NAME_SYSTEM_PROMPT.includes("class"),
      "Should mention classes"
    );
    assert.ok(
      FUNCTION_NAME_SYSTEM_PROMPT.includes("constructor"),
      "Should mention constructors"
    );
  });
});

describe("buildUserPrompt", () => {
  it("includes the current name", () => {
    const context: LLMContext = {
      functionCode: "function a() {}",
      calleeSignatures: [],
      callsites: [],
      usedIdentifiers: new Set()
    };

    const prompt = buildUserPrompt("variableName", context);

    assert.ok(prompt.includes("variableName"), "Should include current name");
  });

  it("includes the function code", () => {
    const context: LLMContext = {
      functionCode: "function calculateSum(a, b) { return a + b; }",
      calleeSignatures: [],
      callsites: [],
      usedIdentifiers: new Set()
    };

    const prompt = buildUserPrompt("a", context);

    assert.ok(prompt.includes("calculateSum"), "Should include function code");
    assert.ok(prompt.includes("return a + b"), "Should include function body");
  });

  it("includes callee signatures when present", () => {
    const context: LLMContext = {
      functionCode: "function a() { fetchUser(); }",
      calleeSignatures: [
        { name: "fetchUser", params: ["userId"], snippet: "async function fetchUser(userId) {" }
      ],
      callsites: [],
      usedIdentifiers: new Set()
    };

    const prompt = buildUserPrompt("a", context);

    assert.ok(prompt.includes("fetchUser"), "Should include callee name");
    assert.ok(prompt.includes("userId"), "Should include callee params");
    assert.ok(prompt.includes("calls these"), "Should explain context");
  });

  it("includes callsites when present", () => {
    const context: LLMContext = {
      functionCode: "function a() {}",
      calleeSignatures: [],
      callsites: ["obj.processData(a)", "result = a()"],
      usedIdentifiers: new Set()
    };

    const prompt = buildUserPrompt("a", context);

    assert.ok(prompt.includes("processData"), "Should include callsite");
    assert.ok(prompt.includes("called like"), "Should explain callsites");
  });

  it("limits callsites to 3", () => {
    const context: LLMContext = {
      functionCode: "function a() {}",
      calleeSignatures: [],
      callsites: ["call1()", "call2()", "call3()", "call4()", "call5()"],
      usedIdentifiers: new Set()
    };

    const prompt = buildUserPrompt("a", context);

    assert.ok(prompt.includes("call1"), "Should include first callsite");
    assert.ok(prompt.includes("call3"), "Should include third callsite");
    assert.ok(!prompt.includes("call4"), "Should not include fourth callsite");
  });

  it("includes used identifiers when present", () => {
    const context: LLMContext = {
      functionCode: "function a() {}",
      calleeSignatures: [],
      callsites: [],
      usedIdentifiers: new Set(["existingVar", "anotherName"])
    };

    const prompt = buildUserPrompt("a", context);

    assert.ok(prompt.includes("existingVar"), "Should include used identifiers");
    assert.ok(prompt.includes("avoid"), "Should mention avoiding names");
  });

  it("limits used identifiers to 50", () => {
    const usedIdentifiers = new Set<string>();
    for (let i = 0; i < 100; i++) {
      usedIdentifiers.add(`var${i}`);
    }

    const context: LLMContext = {
      functionCode: "function a() {}",
      calleeSignatures: [],
      callsites: [],
      usedIdentifiers
    };

    const prompt = buildUserPrompt("a", context);

    // Count occurrences of "var" pattern
    const matches = prompt.match(/var\d+/g) || [];
    assert.ok(matches.length <= 50, "Should limit to 50 identifiers");
  });

  it("wraps code in markdown code block", () => {
    const context: LLMContext = {
      functionCode: "function test() {}",
      calleeSignatures: [],
      callsites: [],
      usedIdentifiers: new Set()
    };

    const prompt = buildUserPrompt("a", context);

    assert.ok(prompt.includes("```javascript"), "Should have code block start");
    assert.ok(prompt.includes("```\n"), "Should have code block end");
  });
});

describe("buildFunctionNamePrompt", () => {
  it("includes the function name", () => {
    const context: LLMContext = {
      functionCode: "function fn() {}",
      calleeSignatures: [],
      callsites: [],
      usedIdentifiers: new Set()
    };

    const prompt = buildFunctionNamePrompt("myFunction", context);

    assert.ok(prompt.includes("myFunction"), "Should include function name");
    assert.ok(prompt.includes("function"), "Should mention function");
  });

  it("includes callee snippets when available", () => {
    const context: LLMContext = {
      functionCode: "function a() { fetchData(); }",
      calleeSignatures: [
        {
          name: "fetchData",
          params: [],
          snippet: "async function fetchData() { return fetch('/api'); }"
        }
      ],
      callsites: [],
      usedIdentifiers: new Set()
    };

    const prompt = buildFunctionNamePrompt("a", context);

    assert.ok(prompt.includes("fetchData"), "Should include callee name");
    assert.ok(prompt.includes("async function"), "Should include snippet");
  });

  it("limits callsites to 5", () => {
    const context: LLMContext = {
      functionCode: "function a() {}",
      calleeSignatures: [],
      callsites: [
        "call1()",
        "call2()",
        "call3()",
        "call4()",
        "call5()",
        "call6()",
        "call7()"
      ],
      usedIdentifiers: new Set()
    };

    const prompt = buildFunctionNamePrompt("a", context);

    assert.ok(prompt.includes("call5"), "Should include fifth callsite");
    assert.ok(!prompt.includes("call6"), "Should not include sixth callsite");
  });

  it("includes used identifiers", () => {
    // Function name prompts now include used identifiers to help avoid conflicts
    const context: LLMContext = {
      functionCode: "function a() {}",
      calleeSignatures: [],
      callsites: [],
      usedIdentifiers: new Set(["existingVar"])
    };

    const prompt = buildFunctionNamePrompt("a", context);

    assert.ok(prompt.includes("existingVar"), "Should include used identifiers");
    assert.ok(prompt.includes("avoid"), "Should mention avoiding names");
  });
});

describe("IDENTIFIER_GRAMMAR", () => {
  it("starts with valid identifier characters", () => {
    assert.ok(
      IDENTIFIER_GRAMMAR.includes("[a-zA-Z_$]"),
      "Should start with letter, underscore, or dollar"
    );
  });

  it("allows alphanumeric continuation", () => {
    assert.ok(
      IDENTIFIER_GRAMMAR.includes("[a-zA-Z0-9_$]"),
      "Should allow alphanumeric continuation"
    );
  });

  it("has length limit", () => {
    assert.ok(
      IDENTIFIER_GRAMMAR.includes("{1,30}"),
      "Should have length limit"
    );
  });
});

describe("JSON_NAME_GRAMMAR", () => {
  it("requires name field", () => {
    assert.ok(
      JSON_NAME_GRAMMAR.includes('"name"'),
      "Should require name field"
    );
  });

  it("defines identifier rule", () => {
    assert.ok(
      JSON_NAME_GRAMMAR.includes("identifier"),
      "Should define identifier rule"
    );
  });

  it("allows whitespace", () => {
    assert.ok(JSON_NAME_GRAMMAR.includes("ws"), "Should handle whitespace");
  });
});

describe("JSON_NAME_REASONING_GRAMMAR", () => {
  it("requires name field", () => {
    assert.ok(
      JSON_NAME_REASONING_GRAMMAR.includes('"name"'),
      "Should require name field"
    );
  });

  it("requires reasoning field", () => {
    assert.ok(
      JSON_NAME_REASONING_GRAMMAR.includes('"reasoning"'),
      "Should require reasoning field"
    );
  });

  it("allows any string for reasoning", () => {
    assert.ok(
      JSON_NAME_REASONING_GRAMMAR.includes('[^"]*'),
      "Should allow any string for reasoning"
    );
  });
});

describe("buildRetryPrompt", () => {
  it("includes the rejected name", () => {
    const context: LLMContext = {
      functionCode: "function a() {}",
      calleeSignatures: [],
      callsites: [],
      usedIdentifiers: new Set(["user"])
    };

    const prompt = buildRetryPrompt("a", "user", context, "already in use");

    assert.ok(prompt.includes("user"), "Should include rejected name");
  });

  it("includes the rejection reason", () => {
    const context: LLMContext = {
      functionCode: "function a() {}",
      calleeSignatures: [],
      callsites: [],
      usedIdentifiers: new Set()
    };

    const prompt = buildRetryPrompt("a", "class", context, "reserved word");

    assert.ok(prompt.includes("reserved word"), "Should include reason");
  });

  it("includes used identifiers with emphasis", () => {
    const context: LLMContext = {
      functionCode: "function a() {}",
      calleeSignatures: [],
      callsites: [],
      usedIdentifiers: new Set(["user", "data", "result"])
    };

    const prompt = buildRetryPrompt("a", "user", context, "already in use");

    assert.ok(prompt.includes("MUST avoid"), "Should emphasize avoiding names");
    assert.ok(prompt.includes("user"), "Should include used identifiers");
  });

  it("asks for a different name", () => {
    const context: LLMContext = {
      functionCode: "function a() {}",
      calleeSignatures: [],
      callsites: [],
      usedIdentifiers: new Set()
    };

    const prompt = buildRetryPrompt("a", "user", context, "in use");

    assert.ok(prompt.includes("DIFFERENT"), "Should ask for different name");
  });
});

describe("buildFunctionRetryPrompt", () => {
  it("includes the rejected function name", () => {
    const context: LLMContext = {
      functionCode: "function a() { return fetch(); }",
      calleeSignatures: [],
      callsites: [],
      usedIdentifiers: new Set(["fetchData"])
    };

    const prompt = buildFunctionRetryPrompt("a", "fetchData", context, "already in use");

    assert.ok(prompt.includes("fetchData"), "Should include rejected name");
  });

  it("mentions function in the prompt", () => {
    const context: LLMContext = {
      functionCode: "function a() {}",
      calleeSignatures: [],
      callsites: [],
      usedIdentifiers: new Set()
    };

    const prompt = buildFunctionRetryPrompt("a", "getData", context, "in use");

    assert.ok(prompt.includes("function"), "Should mention function");
  });

  it("includes used identifiers", () => {
    const context: LLMContext = {
      functionCode: "function a() {}",
      calleeSignatures: [],
      callsites: [],
      usedIdentifiers: new Set(["processData", "handleRequest"])
    };

    const prompt = buildFunctionRetryPrompt("a", "processData", context, "in use");

    assert.ok(prompt.includes("processData"), "Should include used identifiers");
    assert.ok(prompt.includes("handleRequest"), "Should include all used identifiers");
  });
});
