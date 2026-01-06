# Migration Plan

## Overview

Migrate from the current architecture to the new pipeline-based architecture incrementally, maintaining backwards compatibility during the transition.

## Current State

```
src/
├── index.ts                    # CLI entry
├── cli.ts                      # Commander setup
├── unminify.ts                 # Simple plugin pipeline
├── babel-utils.ts              # Babel transform wrapper
├── commands/
│   ├── local.ts                # Local LLM command
│   ├── openai.ts               # OpenAI command
│   ├── gemini.ts               # Gemini command
│   └── download.ts             # Model download
├── plugins/
│   ├── babel/babel.ts          # AST beautification
│   ├── local-llm-rename/       # Local LLM integration
│   │   ├── local-llm-rename.ts
│   │   ├── llama.ts
│   │   ├── visit-all-identifiers.ts  # Core rename logic
│   │   └── ...
│   ├── openai/openai-rename.ts
│   ├── gemini-rename.ts
│   ├── prettier.ts
│   └── webcrack.ts
└── ...
```

### Key Issues to Address

1. **Separate LLM commands** → Unified with `--endpoint`
2. **Sequential identifier processing** → Parallel with ready queue
3. **Underscore conflict resolution** → Smart resolution
4. **No function dependency awareness** → Leaf-first processing
5. **No caching** → Structural hash cache
6. **No source map output** → Standard v3 source maps

## Migration Phases

### Phase 1: Foundation (Week 1-2)

Create new modules alongside existing code. No changes to existing functionality yet.

#### 1.1 Core Types and Interfaces

```bash
# New files
src/types/index.ts              # Shared types
src/llm/types.ts                # LLM provider interface
src/analysis/types.ts           # Function node, graph types
src/cache/types.ts              # Cache entry types
```

#### 1.2 Unified LLM Provider

```bash
# New files
src/llm/provider.ts             # Abstract provider
src/llm/openai-compatible.ts    # OpenAI-compatible implementation
src/llm/local-llama.ts          # Wrapper around existing llama.ts

# Can test independently
npm run test:llm
```

#### 1.3 AST Analysis Module

```bash
# New files
src/analysis/ast-cache.ts       # Cached AST operations
src/analysis/structural-hash.ts # Content-based hashing
src/analysis/function-graph.ts  # Dependency graph

# Tests
src/analysis/__tests__/
```

**Milestone**: New modules pass unit tests, don't affect existing code.

### Phase 2: Processing Pipeline (Week 2-3)

Implement the new processing model.

#### 2.1 Ready Queue Processor

```bash
# New files
src/rename/processor.ts         # Ready queue implementation
src/rename/context-builder.ts   # LLM context extraction

# Reuse existing
src/plugins/local-llm-rename/visit-all-identifiers.ts  # Reference for scope handling
```

#### 2.2 Conflict Resolution

```bash
# New files
src/rename/conflict-resolver.ts # Smart conflict resolution

# Test against existing behavior
src/rename/__tests__/conflict-resolver.test.ts
```

#### 2.3 Source Map Generation

```bash
# New files
src/output/source-map.ts        # Source map generation
src/output/writer.ts            # Output file writing
```

**Milestone**: New pipeline can process a file end-to-end (in tests).

### Phase 3: Integration (Week 3-4)

Wire up new pipeline to CLI.

#### 3.1 New Unified Command

```bash
# New file alongside existing commands
src/commands/humanify.ts        # New unified command

# Register in CLI (additive, doesn't remove existing)
src/cli.ts
```

```typescript
// cli.ts addition
import { humanifyCommand } from './commands/humanify';

program
  .addCommand(humanifyCommand)  // New unified command
  // Existing commands still work
  .addCommand(localCommand)
  .addCommand(openaiCommand)
  .addCommand(geminiCommand);
```

#### 3.2 Cache Integration

```bash
# New files
src/cache/store.ts              # Cache storage
src/commands/cache.ts           # Cache management commands
```

#### 3.3 Library Detection (Optional for MVP)

```bash
# New files
src/analysis/library-detector.ts

# Can be added after initial release
```

**Milestone**: `humanify` command works alongside existing commands.

### Phase 4: Deprecation (Week 4-5)

Deprecate old commands, migrate users.

#### 4.1 Deprecation Warnings

```typescript
// src/commands/openai.ts
export const openaiCommand = new Command('openai')
  .description('DEPRECATED: Use "humanify --endpoint https://api.openai.com/v1" instead')
  .action(async (filename, options) => {
    console.warn(
      chalk.yellow('Warning: "humanify openai" is deprecated. ') +
      'Use "humanify" with --endpoint instead.\n' +
      'See: humanify --help\n'
    );
    // Still works, calls new implementation under the hood
    await runHumanify(filename, {
      endpoint: 'https://api.openai.com/v1',
      ...options
    });
  });
```

#### 4.2 Documentation Update

```bash
# Update README
README.md

# Migration guide
docs/migration-from-v1.md
```

#### 4.3 Version Bump

- Bump to v2.0.0
- Keep deprecated commands working
- Plan removal for v3.0.0

**Milestone**: v2.0.0 released with deprecation warnings.

### Phase 5: Cleanup (Future v3.0.0)

Remove deprecated code.

```bash
# Remove old command files
rm src/commands/local.ts
rm src/commands/openai.ts
rm src/commands/gemini.ts

# Remove old plugins (if fully replaced)
rm -rf src/plugins/local-llm-rename/
rm src/plugins/openai/
rm src/plugins/gemini-rename.ts

# Update CLI
# Remove deprecated command registrations
```

## File-by-File Migration

### Files to Keep (Refactor)

| File | Changes |
|------|---------|
| `src/index.ts` | Minor CLI entry changes |
| `src/cli.ts` | Add new commands, deprecate old |
| `src/plugins/babel/babel.ts` | Keep as-is, used by new pipeline |
| `src/plugins/prettier.ts` | Keep as-is |
| `src/plugins/webcrack.ts` | Keep as-is |
| `src/local-models.ts` | Keep for `--local` support |
| `src/progress.ts` | Refactor for new progress model |

### Files to Replace

| Old File | New File(s) |
|----------|-------------|
| `src/unminify.ts` | `src/pipeline/index.ts` |
| `src/plugins/local-llm-rename/visit-all-identifiers.ts` | `src/rename/processor.ts`, `src/analysis/function-graph.ts` |
| `src/plugins/local-llm-rename/llama.ts` | `src/llm/local-llama.ts` |
| `src/plugins/openai/openai-rename.ts` | `src/llm/openai-compatible.ts` |
| `src/plugins/gemini-rename.ts` | (Use OpenAI-compatible with Gemini endpoint) |
| `src/commands/local.ts` | `src/commands/humanify.ts` |
| `src/commands/openai.ts` | `src/commands/humanify.ts` |
| `src/commands/gemini.ts` | `src/commands/humanify.ts` |

### New Files

```
src/
├── types/
│   └── index.ts
├── llm/
│   ├── types.ts
│   ├── provider.ts
│   ├── openai-compatible.ts
│   └── local-llama.ts
├── analysis/
│   ├── types.ts
│   ├── ast-cache.ts
│   ├── structural-hash.ts
│   ├── function-graph.ts
│   └── library-detector.ts
├── rename/
│   ├── processor.ts
│   ├── context-builder.ts
│   └── conflict-resolver.ts
├── cache/
│   ├── types.ts
│   └── store.ts
├── output/
│   ├── source-map.ts
│   └── writer.ts
├── pipeline/
│   ├── index.ts
│   └── context.ts
└── commands/
    ├── humanify.ts
    └── cache.ts
```

## Testing Strategy

### Unit Tests

Each new module gets comprehensive unit tests:

```bash
src/analysis/__tests__/structural-hash.test.ts
src/analysis/__tests__/function-graph.test.ts
src/rename/__tests__/processor.test.ts
src/rename/__tests__/conflict-resolver.test.ts
src/llm/__tests__/openai-compatible.test.ts
src/cache/__tests__/store.test.ts
```

### Integration Tests

Test the full pipeline:

```bash
src/__tests__/pipeline.integration.test.ts
```

### Regression Tests

Ensure new implementation produces same/better results:

```typescript
// Compare old vs new implementation
describe('regression', () => {
  const testCases = loadTestCases('./fixtures/');

  for (const tc of testCases) {
    it(`should handle ${tc.name}`, async () => {
      const oldResult = await oldImplementation(tc.input);
      const newResult = await newImplementation(tc.input);

      // New should be at least as good
      expect(newResult.renamedCount).toBeGreaterThanOrEqual(oldResult.renamedCount);

      // No underscore spam
      expect(newResult.code).not.toMatch(/_{3,}/);
    });
  }
});
```

### E2E Tests

```bash
# Test CLI end-to-end
src/test/e2e/cli.test.ts
```

## Rollback Plan

If issues arise after v2.0.0 release:

1. **Old commands still work** - Users can use `humanify openai` instead of new `humanify`
2. **Feature flag** - Add `--legacy` flag to force old implementation
3. **Quick patch** - Can release v2.0.1 with fixes without full rollback

## Success Criteria

### Phase 1
- [ ] All new modules have >80% test coverage
- [ ] No changes to existing functionality

### Phase 2
- [ ] New pipeline processes test fixtures correctly
- [ ] Performance is equal or better than current

### Phase 3
- [ ] `humanify` command works for all provider types
- [ ] Source maps validate correctly
- [ ] Cache hits work across runs

### Phase 4
- [ ] Deprecation warnings appear for old commands
- [ ] Documentation updated
- [ ] v2.0.0 released

### Phase 5 (Future)
- [ ] Old code removed
- [ ] v3.0.0 released
