# Humanify Architecture Overview

## Goal

Transform minified JavaScript into human-readable code by:
1. Unpacking bundled code (via webcrack)
2. Identifying library code vs novel/application code
3. Intelligently renaming identifiers using LLMs
4. Outputting humanified code with source maps

## Design Principles

### Leaf-First Processing
Process functions starting from "leaves" (functions that only call external/library code) and work upward. This ensures that when we process a function, all the functions it calls have already been humanified, giving the LLM maximum context.

### Parallel Processing
Functions with no dependencies on each other can be processed in parallel. We use a ready queue that tracks which functions have all their dependencies satisfied.

### Scope-Aware Renaming
Each function only renames its own bindings (parameters, local variables). Parent scope variables are the parent function's responsibility. This ensures parallel processing is safe.

### Source Maps as First-Class Output
Generate standard v3 source maps that work with browser devtools, error trackers, and other tools. Optionally output only a source map (no code rewrite needed).

### Structural Caching
Cache humanification results by structural hash (AST shape, ignoring names). This enables:
- Incremental processing of large files
- Cross-version reuse (same function in v2 reuses v1's humanification)

## Pipeline Stages

```
┌─────────┐   ┌─────────┐   ┌──────────┐   ┌─────────┐   ┌────────┐   ┌────────┐
│ UNPACK  │──▶│ ANALYZE │──▶│ CLASSIFY │──▶│ PROCESS │──▶│ RENAME │──▶│ OUTPUT │
└─────────┘   └─────────┘   └──────────┘   └─────────┘   └────────┘   └────────┘
```

### 1. Unpack
- Use webcrack to unpack bundled/webpacked code
- Split into individual modules/files

### 2. Analyze
- Parse each file to AST via Babel
- Build function dependency graph
- Compute structural hashes for each function
- Cache AST and scope information

### 3. Classify
- Identify known libraries (React, lodash, etc.) by signatures
- Mark library functions as "external" (don't process)
- Only process novel/application code

### 4. Process (Ready Queue)
- Initialize queue with leaf functions (no internal dependencies)
- Process functions in parallel (with concurrency limit)
- As functions complete, check for newly ready functions
- Continue until all functions processed

### 5. Rename
- For each function, extract context from current AST state
- Query LLM for identifier rename suggestions
- Apply renames immediately (safe due to scope isolation)
- Track original positions for source map generation

### 6. Output
- Generate humanified code from final AST
- Generate source map from tracked renames
- Support: code only, source map only, or both

## Directory Structure

```
src/
├── pipeline/
│   ├── index.ts           # Pipeline orchestrator
│   ├── stages/            # Individual pipeline stages
│   └── context.ts         # Shared pipeline context
├── analysis/
│   ├── ast-cache.ts       # Cached AST + scope management
│   ├── function-graph.ts  # Function dependency graph
│   ├── structural-hash.ts # Content-based function identity
│   └── library-detector.ts
├── rename/
│   ├── processor.ts       # Ready queue + parallel processing
│   ├── context-builder.ts # Build LLM context
│   └── conflict-resolver.ts
├── llm/
│   ├── provider.ts        # Unified LLM interface
│   ├── openai-compatible.ts
│   └── prompts/
├── cache/
│   ├── store.ts           # Structural hash → rename mapping
│   └── source-map.ts      # Source map generation
└── cli/
    └── commands/
```

## Specification Documents

| Document | Description |
|----------|-------------|
| [01 - Function Processing](./01-function-processing.md) | Ready queue, parallel execution, dependency graph |
| [02 - LLM Integration](./02-llm-integration.md) | Provider interface, prompts, rate limiting |
| [03 - Caching & Source Maps](./03-caching-source-maps.md) | Structural hashing, cache format, source map generation |
| [04 - Conflict Resolution](./04-conflict-resolution.md) | Smart naming strategies (no `_` spam) |
| [05 - Library Detection](./05-library-detection.md) | Identifying and skipping library code |
| [06 - CLI Design](./06-cli-design.md) | Unified command interface, options, examples |
| [07 - Migration Plan](./07-migration-plan.md) | Phased migration from current codebase |
