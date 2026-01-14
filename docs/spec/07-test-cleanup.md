# Test Cleanup: Remove LLM-Dependent Tests

## Problem

Several tests currently make actual LLM calls (to local models via `node-llama-cpp`), which causes issues:

1. **Flaky results** - LLM outputs are non-deterministic, even with fixed seeds
2. **Slow execution** - Local model inference takes seconds per call
3. **Environment-dependent** - Requires specific models to be downloaded
4. **False failures** - Tests fail due to LLM judgment, not code bugs (e.g., "UNREADABLE" vs "GOOD")

## Affected Tests

### `src/test/local.e2etest.ts`

Uses "LLM-as-a-judge" pattern to verify humanify improves readability:

```typescript
// Asks LLM to rate code as EXCELLENT/GOOD/UNREADABLE
const fileIsMinified = async (filename: string) => {
  const prompt = await testPrompt();
  return await prompt(
    `Rate readability: "EXCELLENT", "GOOD" or "UNREADABLE"`,
    await readFile(filename, "utf-8"),
    gbnf`${/("EXCELLENT" | "GOOD" | "UNREADABLE") [^.]+/}.`
  );
};
```

**Problem**: The test expects minified code to be rated "UNREADABLE" but LLMs may rate simple minified code as "GOOD".

### Other potential LLM-dependent tests

- Any test using `testPrompt()` from `src/test/test-prompt.ts`
- Any test importing from `src/plugins/local-llm-rename/`

## Recommended Changes

### 1. Replace LLM-as-judge with deterministic checks

Instead of asking an LLM if code is readable, use static analysis:

```typescript
// Before: LLM judges readability
const rating = await prompt("Rate readability...", code);
expect(rating).toBe("UNREADABLE");

// After: Check concrete properties
function isMinified(code: string): boolean {
  const ast = parse(code);
  const identifiers = collectIdentifiers(ast);

  // Minified code has short, meaningless names
  const avgLength = identifiers.reduce((sum, id) => sum + id.length, 0) / identifiers.length;
  const singleCharCount = identifiers.filter(id => id.length === 1).length;

  return avgLength < 3 || singleCharCount / identifiers.length > 0.5;
}

// Test
expect(isMinified(inputCode)).toBe(true);
expect(isMinified(outputCode)).toBe(false);
```

### 2. Mock LLM providers in integration tests

For tests that need to verify the rename pipeline works:

```typescript
const mockProvider: LLMProvider = {
  async suggestName(name: string, context: LLMContext) {
    // Deterministic renaming based on context
    if (context.functionCode.includes("substring")) {
      return { name: "chunkString" };
    }
    return { name: name + "Renamed" };
  }
};

// Use mock in test
const processor = new RenameProcessor(graph, mockProvider);
```

### 3. Separate test categories

```
npm run test:unit     # Fast, no LLM, always deterministic
npm run test:e2e      # Integration tests with mocked LLM
npm run test:llm      # Optional: actual LLM tests (CI-skip by default)
```

### 4. Snapshot testing for LLM output verification

If we need to verify LLM integration works end-to-end:

```typescript
// Run once with real LLM, save snapshot
// Future runs compare against snapshot (no LLM call)
test("humanify produces expected output", async () => {
  const output = await humanify("fixtures/example.min.js");
  expect(output).toMatchSnapshot();
});
```

## Migration Plan

1. **Identify all LLM-dependent tests** - Grep for `testPrompt`, `llama(`, actual API calls
2. **Create mock providers** - Build reusable mocks that return deterministic results
3. **Rewrite e2e tests** - Replace LLM-as-judge with static analysis or mocks
4. **Move real LLM tests** - Keep a small set of "smoke tests" that run against real LLMs, but skip in CI
5. **Update CI configuration** - Ensure CI only runs deterministic tests

## Success Criteria

- All tests in `npm run test:unit` and `npm run test:e2e` pass deterministically
- No tests require downloaded models or API keys
- Test suite completes in under 30 seconds (excluding optional LLM tests)
- Zero flaky tests in CI
