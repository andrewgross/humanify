# Legacy Code Migration

## Overview

Replace the legacy sequential renaming system with the new `RenameProcessor`-based system. This enables parallel processing, better context for LLM calls, and proper dependency ordering.

## Current State

### Legacy System (to remove)

```
CLI Command (openai/gemini/local)
    ↓
unminify() - runs plugins sequentially
    ↓
*Rename plugin (openaiRename/geminiRename/localRename)
    ↓
visitAllIdentifiers() - processes identifiers one at a time
    ↓
Individual LLM calls with limited context
```

**Problems:**
- Sequential processing (no parallelism)
- No dependency ordering (can't provide callee context)
- Each identifier processed in isolation
- No retry with LLM feedback on conflicts
- No metrics/observability

### New System (implemented, not wired up)

```
CLI Command
    ↓
unminify() - runs plugins
    ↓
New rename plugin
    ↓
buildFunctionGraph() - analyze dependencies
    ↓
RenameProcessor.processAll() - parallel, dependency-ordered
    ↓
LLMProvider with retry, rate limiting, metrics
```

**Benefits:**
- Parallel processing (configurable concurrency)
- Leaf-first ordering (callees named before callers)
- Rich context (callee signatures, callsites)
- LLM retry on conflicts (up to 9 attempts)
- Metrics and progress tracking

## Files to Remove

### Legacy Plugin System

| File | Reason |
|------|--------|
| `src/plugins/local-llm-rename/visit-all-identifiers.ts` | Replaced by RenameProcessor |
| `src/plugins/local-llm-rename/visit-all-identifiers.test.ts` | Tests for removed code |
| `src/plugins/local-llm-rename/unminify-variable-name.ts` | Legacy prompt/rename logic |
| `src/plugins/local-llm-rename/unminify-variable-name.llmtest.ts` | LLM test for removed code |
| `src/plugins/local-llm-rename/define-filename.ts` | Not needed (was for filename guessing) |
| `src/plugins/local-llm-rename/define-filename.llmtest.ts` | LLM test for removed code |
| `src/plugins/local-llm-rename/local-llm-rename.ts` | Legacy plugin wrapper |
| `src/plugins/local-llm-rename/phi-jinja-template.ts` | Legacy prompt template |
| `src/plugins/openai/openai-rename.ts` | Legacy OpenAI plugin |
| `src/plugins/gemini-rename.ts` | Legacy Gemini plugin |

### Files to Keep (in local-llm-rename)

| File | Reason |
|------|--------|
| `gbnf.ts` | Grammar utilities - useful for local models |
| `gbnf.test.ts` | Tests for grammar utilities |
| `llama.ts` | Local llama.cpp integration - needed for local provider |

## New Unified Plugin

Create a single rename plugin that works with any LLM provider:

```typescript
// src/plugins/rename.ts

import { parseSync } from "@babel/core";
import * as babelGenerator from "@babel/generator";
import { buildFunctionGraph } from "../analysis/function-graph.js";
import { RenameProcessor } from "../rename/processor.js";
import { MetricsTracker, formatMetricsCompact } from "../llm/metrics.js";
import type { LLMProvider } from "../llm/types.js";

interface RenamePluginOptions {
  provider: LLMProvider;
  concurrency?: number;
  onProgress?: (message: string) => void;
}

export function createRenamePlugin(options: RenamePluginOptions) {
  const { provider, concurrency = 50, onProgress } = options;

  return async (code: string): Promise<string> => {
    const ast = parseSync(code, { sourceType: "module" });
    if (!ast) throw new Error("Failed to parse code");

    const functions = buildFunctionGraph(ast, "input.js");

    const metrics = new MetricsTracker({
      onMetrics: (m) => onProgress?.(formatMetricsCompact(m))
    });

    const processor = new RenameProcessor(ast);
    await processor.processAll(functions, provider, {
      concurrency,
      metrics
    });

    const output = babelGenerator.default(ast);
    return output.code;
  };
}
```

## Updated CLI Commands

### OpenAI Command

```typescript
// src/commands/openai.ts

import { createRenamePlugin } from "../plugins/rename.js";
import { createOpenAIProvider } from "../llm/openai-compatible.js";
import { withRateLimit } from "../llm/rate-limiter.js";

export const openai = cli()
  .name("openai")
  .option("-m, --model <model>", "Model to use", "gpt-4o-mini")
  .option("-c, --concurrency <n>", "Concurrent requests", "50")
  .option("-k, --apiKey <key>", "API key")
  .option("--baseURL <url>", "API base URL")
  .argument("input", "Input file")
  .action(async (filename, opts) => {
    const baseProvider = createOpenAIProvider({
      apiKey: opts.apiKey ?? env("OPENAI_API_KEY"),
      baseURL: opts.baseURL,
      model: opts.model
    });

    const provider = withRateLimit(baseProvider, {
      maxConcurrent: parseInt(opts.concurrency)
    });

    await unminify(filename, opts.outputDir, [
      babel,
      createRenamePlugin({
        provider,
        concurrency: parseInt(opts.concurrency),
        onProgress: console.log
      }),
      prettier
    ]);
  });
```

### Local Command

```typescript
// src/commands/local.ts

import { createRenamePlugin } from "../plugins/rename.js";
import { createLocalProvider } from "../llm/local-llama.js";

export const local = cli()
  .name("local")
  .option("-m, --model <model>", "Model path or name")
  .option("-c, --concurrency <n>", "Concurrent requests", "1") // Lower for local
  .argument("input", "Input file")
  .action(async (filename, opts) => {
    const provider = await createLocalProvider({
      model: opts.model
    });

    await unminify(filename, opts.outputDir, [
      babel,
      createRenamePlugin({
        provider,
        concurrency: parseInt(opts.concurrency),
        onProgress: console.log
      }),
      prettier
    ]);
  });
```

## E2E Tests with Mocked LLM

### New Test File

```typescript
// src/test/rename.e2etest.ts

import { describe, it } from "node:test";
import assert from "node:assert";
import { createRenamePlugin } from "../plugins/rename.js";
import type { LLMProvider } from "../llm/types.js";

describe("Rename E2E", () => {
  it("transforms minified code to readable code", async () => {
    const minified = `function a(b,c){return b+c}function d(){return a(1,2)}`;

    const mockProvider: LLMProvider = {
      async suggestName(name) {
        const renames: Record<string, string> = {
          a: "addNumbers",
          b: "firstNumber",
          c: "secondNumber",
          d: "calculateSum"
        };
        return { name: renames[name] || name };
      }
    };

    const plugin = createRenamePlugin({
      provider: mockProvider,
      concurrency: 1
    });

    const result = await plugin(minified);

    assert.ok(result.includes("addNumbers"), "Should rename function a");
    assert.ok(result.includes("firstNumber"), "Should rename param b");
    assert.ok(result.includes("secondNumber"), "Should rename param c");
    assert.ok(result.includes("calculateSum"), "Should rename function d");
    assert.ok(!result.includes("function a("), "Original name should be gone");
  });

  it("handles nested functions with shared scope", async () => {
    const code = `
      function a() {
        var b = 1;
        function c() { return b; }
        return c();
      }
    `;

    const callOrder: string[] = [];
    const mockProvider: LLMProvider = {
      async suggestName(name) {
        callOrder.push(name);
        return { name: name + "Renamed" };
      }
    };

    const plugin = createRenamePlugin({
      provider: mockProvider,
      concurrency: 1
    });

    await plugin(code);

    // Parent function should be processed before child
    const aIndex = callOrder.indexOf("a");
    const cIndex = callOrder.indexOf("c");
    assert.ok(aIndex < cIndex, "Parent should be processed before child");
  });

  it("processes functions in dependency order", async () => {
    const code = `
      function caller() { return leaf(); }
      function leaf() { return 42; }
    `;

    const callOrder: string[] = [];
    const mockProvider: LLMProvider = {
      async suggestName(name) {
        callOrder.push(name);
        return { name: name + "Renamed" };
      }
    };

    const plugin = createRenamePlugin({
      provider: mockProvider,
      concurrency: 1
    });

    await plugin(code);

    // Leaf should be processed before caller
    const leafIndex = callOrder.indexOf("leaf");
    const callerIndex = callOrder.indexOf("caller");
    assert.ok(leafIndex < callerIndex, "Leaf should be processed first");
  });

  it("retries on name conflicts", async () => {
    const code = `function a(b) { return b; }`;

    let attempts = 0;
    const mockProvider: LLMProvider = {
      async suggestName(name) {
        if (name === "b") {
          attempts++;
          // First attempt conflicts, second succeeds
          if (attempts === 1) return { name: "a" }; // Conflicts with function name
          return { name: "input" };
        }
        return { name: "myFunction" };
      },
      async retrySuggestName(name, rejected, reason) {
        return { name: "inputValue" };
      }
    };

    const plugin = createRenamePlugin({
      provider: mockProvider,
      concurrency: 1
    });

    const result = await plugin(code);

    // Should have resolved the conflict
    assert.ok(!result.includes("function a(a)"), "Should not have conflicting names");
  });
});
```

### Keep Real LLM Tests as Integration Tests

Move existing E2E tests to a separate integration test directory:

```
src/test/
  ├── rename.e2etest.ts      # Mocked - runs in CI
  └── integration/
      ├── openai.test.ts     # Real API - manual run
      ├── local.test.ts      # Real local model - manual run
      └── gemini.test.ts     # Real API - manual run
```

## Migration Steps

### Phase 1: Add New Plugin
1. Create `src/plugins/rename.ts` with the new unified plugin
2. Add E2E tests with mocked LLM
3. Verify tests pass

### Phase 2: Update CLI Commands
1. Update `openai.ts` to use new plugin
2. Update `local.ts` to use new plugin
3. Update `gemini.ts` to use new plugin
4. Verify CLI works end-to-end

### Phase 3: Remove Legacy Code
1. Delete legacy plugin files (see table above)
2. Remove `--contextSize` CLI option (no longer needed)
3. Update imports and exports

### Phase 4: Update Tests
1. Move real LLM tests to `integration/` directory
2. Update test scripts in package.json:
   ```json
   {
     "test:unit": "find src -name '*.test.ts' | xargs tsx --test",
     "test:e2e": "find src -name '*.e2etest.ts' | xargs tsx --test",
     "test:integration": "find src/test/integration -name '*.test.ts' | xargs tsx --test"
   }
   ```

## Verification Checklist

- [ ] All unit tests pass
- [ ] E2E tests with mocked LLM pass
- [ ] CLI `humanify openai <file>` works
- [ ] CLI `humanify local <file>` works
- [ ] CLI `humanify gemini <file>` works
- [ ] `--concurrency` flag is respected
- [ ] Progress output shows metrics
- [ ] No references to removed files remain
