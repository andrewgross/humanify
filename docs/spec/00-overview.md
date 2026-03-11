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
- Build unified dependency graph combining functions and module-level bindings
- Detect cross-type dependencies (function→module var, module var→function)
- Compute structural hashes for each function
- Cache AST and scope information

### 3. Classify
- Identify known libraries (React, lodash, etc.) by signatures
- Mark library functions as "external" (don't process)
- Only process novel/application code

### 4. Process (Unified Ready Queue)
- Initialize queue with leaf nodes (functions and module-level bindings with no internal dependencies)
- Process all node types in parallel (with concurrency limit)
- Function nodes get function-specific prompts; module binding nodes get batched module-level prompts
- As nodes complete, check for newly ready nodes (including cross-type dependents)
- Continue until all nodes processed

### 5. Rename
- For functions: extract context from current AST state, query LLM for identifier rename suggestions
- For module-level bindings: batch into groups of 5, include declaration + assignment/usage context snippets, use proximity-windowed usedNames
- Apply renames immediately (safe due to scope isolation)
- Track original positions for source map generation

### 6. Output
- Generate humanified code from final AST
- Generate source map from tracked renames
- Support: code only, source map only, or both

## Directory Structure

```
src/
├── analysis/
│   ├── function-graph.ts      # Function dependency graph builder
│   ├── structural-hash.ts     # Content-based structural hashing
│   ├── function-fingerprint.ts # Multi-resolution fingerprinting
│   └── types.ts               # FunctionNode, ModuleBindingNode, RenameNode, etc.
├── rename/
│   ├── processor.ts           # Ready queue + parallel processing (functions + module bindings)
│   └── context-builder.ts     # Build LLM context for rename
├── utils/
│   └── concurrency.ts         # Shared concurrency limiter
├── library-detection/
│   ├── detector.ts            # Multi-layer library detection
│   ├── comment-patterns.ts    # Banner/license comment matching
│   ├── comment-regions.ts     # Intra-file region detection (Layer 3)
│   ├── types.ts               # DetectionResult, MixedFileDetection
│   └── index.ts               # Public API
├── checkpoint/                # (planned — spec 16)
│   ├── store.ts               # SQLite-backed checkpoint persistence
│   └── index.ts               # Public API
├── llm/
│   ├── openai-compatible.ts   # OpenAI-compatible provider
│   ├── types.ts               # LLMProvider interface
│   ├── metrics.ts             # Processing metrics tracker
│   ├── validation.ts          # Identifier validation + conflict resolution
│   └── prompts/               # LLM prompt templates
├── plugins/
│   ├── rename.ts              # Rename plugin (wires graph → processor)
│   ├── webcrack.ts            # Webcrack integration
│   └── babel/                 # Babel transform plugins
├── commands/
│   └── unified.ts             # CLI command definitions
├── unminify.ts                # Top-level orchestrator (file loop)
├── cli.ts                     # CLI entry point
└── source-map-writer.ts       # Source map generation
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
| [07 - Test Cleanup](./07-test-cleanup.md) | Test infrastructure cleanup |
| [08 - LLM Conflict Resolution](./08-llm-conflict-resolution.md) | LLM-aware naming conflict resolution |
| [09 - Legacy Migration](./09-legacy-migration.md) | Legacy code migration strategy |
| [10 - Callsite Indexing](./10-callsite-indexing.md) | Pre-computed call site information |
| [11 - Batched Renaming](./11-batched-renaming.md) | Batch LLM rename requests for efficiency |
| [12 - Cross-Version Fingerprinting](./12-cross-version-fingerprinting.md) | Content-based function identity across versions |
| [13 - E2E Validation Harness](./13-e2e-validation-harness.md) | End-to-end testing infrastructure |
| [14 - Webcrack Source Map Migration](./14-webcrack-source-map-migration.md) | Source map chaining with webcrack |
| [15 - Fingerprint Benchmarking](./15-fingerprint-benchmarking.md) | Benchmarking fingerprint matching accuracy |
| [16 - Resumable Processing](./16-resumable-processing.md) | SQLite-based checkpoint/resume for large bundles |
| [20 - Unified Rename Graph](./20-unified-rename-graph.md) | Single-pass parallel processing of functions + module-level bindings |
| [24 - Bundler Detection](./24-bundler-detection.md) | Identify bundler + minifier, route to appropriate adapters |
| [25 - Structure Recovery](./25-structure-recovery.md) | Recover module boundaries via constraints (mutation affinity, scope, deconflict) |
