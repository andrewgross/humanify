# Caching and Source Maps

## Overview

Two related but distinct features:

1. **Structural Caching** - Reuse humanification results for identical function structures across runs and file versions
2. **Source Maps** - Standard v3 source maps for tooling integration (devtools, error trackers)

## Structural Hashing

### Goal

Identify "the same function" even when minified names differ between versions:

```javascript
// v1.min.js
function a(b, c) { return b + c; }

// v2.min.js (same function, different minified names)
function x(y, z) { return y + z; }
```

Both should produce the same structural hash.

### Algorithm

```typescript
// src/analysis/structural-hash.ts

import { createHash } from 'crypto';
import traverse from '@babel/traverse';
import * as t from '@babel/types';

function computeStructuralHash(fnNode: t.Function): string {
  // Clone to avoid mutating original
  const normalized = normalizeAST(t.cloneNode(fnNode, true));
  const serialized = JSON.stringify(normalized);
  return createHash('sha256').update(serialized).digest('hex').slice(0, 16);
}

function normalizeAST(node: t.Node): t.Node {
  let placeholderCounter = 0;
  const identifierMap = new Map<string, string>();

  function getPlaceholder(name: string): string {
    if (!identifierMap.has(name)) {
      identifierMap.set(name, `$${placeholderCounter++}`);
    }
    return identifierMap.get(name)!;
  }

  traverse(node, {
    // Normalize all identifiers to positional placeholders
    Identifier(path) {
      path.node.name = getPlaceholder(path.node.name);
    },

    // Normalize string literals (keep length info, not content)
    StringLiteral(path) {
      path.node.value = `__STR_${path.node.value.length}__`;
    },

    // Normalize numbers (keep magnitude bucket)
    NumericLiteral(path) {
      const magnitude = Math.floor(Math.log10(Math.abs(path.node.value) + 1));
      path.node.value = magnitude;
    },

    // Remove location info
    enter(path) {
      delete path.node.loc;
      delete path.node.start;
      delete path.node.end;
    }
  });

  return node;
}
```

### Placeholder Assignment Order

Placeholders are assigned in AST traversal order (depth-first, left-to-right). This ensures deterministic hashing:

```javascript
function foo(a, b) {    // $0 = foo, $1 = a, $2 = b
  const c = a + b;      // $3 = c
  return c;
}

// Normalized:
function $0($1, $2) {
  const $3 = $1 + $2;
  return $3;
}
```

## Cache Storage

### Format

```typescript
// ~/.humanify/cache/index.json

interface CacheStore {
  version: 1;
  entries: {
    [structuralHash: string]: CacheEntry;
  };
}

interface CacheEntry {
  // Mapping from placeholder to humanified name
  names: Record<string, string>;  // { "$0": "calculateSum", "$1": "price", ... }

  // Metadata
  model: string;         // LLM used
  confidence: number;    // Average confidence
  created: string;       // ISO timestamp
  accessCount: number;   // For LRU eviction
  lastAccess: string;    // ISO timestamp
}
```

### Operations

```typescript
// src/cache/store.ts

class HumanifyCache {
  private store: CacheStore;
  private cachePath: string;

  // Session statistics for metrics
  private sessionHits = 0;
  private sessionMisses = 0;

  async load(cachePath: string): Promise<void> {
    this.cachePath = cachePath;
    try {
      const data = await fs.readFile(cachePath, 'utf-8');
      this.store = JSON.parse(data);
    } catch {
      this.store = { version: 1, entries: {} };
    }
  }

  lookup(hash: string): CacheEntry | null {
    const entry = this.store.entries[hash];
    if (entry) {
      entry.accessCount++;
      entry.lastAccess = new Date().toISOString();
      this.sessionHits++;
      return entry;
    }
    this.sessionMisses++;
    return null;
  }

  store(hash: string, entry: CacheEntry): void {
    this.store.entries[hash] = entry;
  }

  async save(): Promise<void> {
    await fs.writeFile(
      this.cachePath,
      JSON.stringify(this.store, null, 2)
    );
  }

  // Get session statistics
  getSessionStats(): CacheSessionStats {
    return {
      hits: this.sessionHits,
      misses: this.sessionMisses,
      hitRate: this.sessionHits / (this.sessionHits + this.sessionMisses) || 0,
      totalEntries: Object.keys(this.store.entries).length
    };
  }

  // Evict old/unused entries
  prune(maxEntries: number = 10000): number {
    const entries = Object.entries(this.store.entries);
    if (entries.length <= maxEntries) return 0;

    // Sort by last access, remove oldest
    entries.sort((a, b) =>
      new Date(a[1].lastAccess).getTime() -
      new Date(b[1].lastAccess).getTime()
    );

    const toRemove = entries.slice(0, entries.length - maxEntries);
    for (const [hash] of toRemove) {
      delete this.store.entries[hash];
    }
    return toRemove.length;
  }
}

interface CacheSessionStats {
  hits: number;
  misses: number;
  hitRate: number;      // 0.0 - 1.0
  totalEntries: number;
}
```

### Cache Metrics Integration

The cache integrates with the metrics system to track hits/misses:

```typescript
// Add to ProcessingMetrics
interface ProcessingMetrics {
  // ... existing fields
  cache?: CacheMetrics;
}

interface CacheMetrics {
  hits: number;           // Functions served from cache
  misses: number;         // Functions needing LLM processing
  hitRate: number;        // Percentage (0-100)
  newEntries: number;     // New entries added this session
}
```

Progress display shows cache effectiveness:

```
Cache lookup...
  ✓ Cache hits: 142/188 (75%)
  ✓ Need to process: 46 functions
```

### Applying Cached Results

```typescript
function applyCachedRenames(
  fn: FunctionNode,
  cached: CacheEntry
): RenameDecision[] {
  const decisions: RenameDecision[] = [];

  // Rebuild placeholder mapping for this specific function
  const placeholderToBinding = buildPlaceholderMapping(fn.path.node);

  for (const [placeholder, humanName] of Object.entries(cached.names)) {
    const binding = placeholderToBinding.get(placeholder);
    if (binding) {
      decisions.push({
        originalPosition: binding.path.node.loc.start,
        path: binding.path,
        originalName: binding.name,
        newName: humanName,
        functionId: fn.sessionId,
        fromCache: true
      });
    }
  }

  return decisions;
}
```

## Source Maps

### Generation

```typescript
// src/output/source-map.ts

import { SourceMapGenerator } from 'source-map';

function generateSourceMap(
  filePath: string,
  renames: RenameDecision[]
): RawSourceMap {
  const generator = new SourceMapGenerator({
    file: filePath
  });

  // Collect all humanified names
  const names = [...new Set(renames.map(r => r.newName))];

  // Add mappings for each renamed identifier
  for (const rename of renames) {
    generator.addMapping({
      generated: {
        line: rename.originalPosition.line,
        column: rename.originalPosition.column
      },
      source: filePath,
      original: {
        line: rename.originalPosition.line,
        column: rename.originalPosition.column
      },
      name: rename.newName
    });
  }

  return generator.toJSON();
}
```

### Source Map Structure

Standard v3 source map format:

```json
{
  "version": 3,
  "file": "bundle.min.js",
  "sources": ["bundle.min.js"],
  "sourcesContent": ["...original minified code..."],
  "names": ["calculateSum", "price", "quantity", "fetchUser", "..."],
  "mappings": "AAAA,SAASA,EAAE,CAACC,CAAD,EAAOC,CAAP,..."
}
```

### Output Modes

```typescript
// src/output/writer.ts

type OutputMode =
  | 'code'              // Just humanified code
  | 'sourcemap'         // Just source map (no code changes)
  | 'both'              // Code + external .map file
  | 'inline';           // Code with embedded source map

async function writeOutput(
  code: string,
  sourceMap: RawSourceMap,
  inputPath: string,
  outputPath: string,
  mode: OutputMode
): Promise<void> {
  switch (mode) {
    case 'code':
      await fs.writeFile(outputPath, code);
      break;

    case 'sourcemap':
      // Write source map next to original file
      await fs.writeFile(
        `${inputPath}.map`,
        JSON.stringify(sourceMap)
      );
      break;

    case 'both':
      // Write code with reference to external map
      await fs.writeFile(
        outputPath,
        code + `\n//# sourceMappingURL=${path.basename(outputPath)}.map`
      );
      await fs.writeFile(
        `${outputPath}.map`,
        JSON.stringify(sourceMap)
      );
      break;

    case 'inline':
      // Embed source map in code
      const encoded = Buffer.from(JSON.stringify(sourceMap)).toString('base64');
      await fs.writeFile(
        outputPath,
        code + `\n//# sourceMappingURL=data:application/json;base64,${encoded}`
      );
      break;
  }
}
```

## CLI Options

```bash
# Default: humanified code + external source map
humanify input.min.js -o output.js
# Creates: output.js, output.js.map

# Source map only (apply to original without rewriting)
humanify input.min.js --source-map-only
# Creates: input.min.js.map

# Inline source map
humanify input.min.js --inline-source-map -o output.js
# Creates: output.js (with embedded map)

# With caching
humanify input.min.js --cache ~/.humanify/cache -o output.js
# Uses cached results when available

# Cache stats
humanify cache stats
# Shows: 1,234 entries, 89% hit rate, 12MB

# Clear cache
humanify cache clear
```

## Cross-Version Workflow

```bash
# First run - full processing
humanify app-v1.min.js --cache ./cache -o v1/app.js
# Output: Processed 912 functions in 45s

# Second run - same codebase, different version
humanify app-v2.min.js --cache ./cache -o v2/app.js
# Output: Matched 847/912 functions from cache (92%)
#         Processed 65 new functions in 3s

# The cache enables fast iteration as code changes
```

## Source Map Chaining

If the input already has a source map (e.g., from TypeScript):

```bash
humanify bundle.min.js --input-source-map bundle.min.js.map -o humanified.js
```

This creates a composed source map:
- Original: `app.ts:42` → `bundle.min.js:1` (build map)
- Humanify: `bundle.min.js:1` → name: "fetchUserProfile"
- Combined: `app.ts:42` → `humanified.js:15`, name: "fetchUserProfile"

```typescript
import { SourceMapConsumer, SourceMapGenerator } from 'source-map';

async function chainSourceMaps(
  originalMap: RawSourceMap,
  humanifyMap: RawSourceMap
): Promise<RawSourceMap> {
  const original = await new SourceMapConsumer(originalMap);
  const humanify = await new SourceMapConsumer(humanifyMap);

  const combined = new SourceMapGenerator();

  humanify.eachMapping(mapping => {
    // Look up original position
    const originalPos = original.originalPositionFor({
      line: mapping.generatedLine,
      column: mapping.generatedColumn
    });

    if (originalPos.source) {
      combined.addMapping({
        generated: {
          line: mapping.generatedLine,
          column: mapping.generatedColumn
        },
        source: originalPos.source,
        original: {
          line: originalPos.line,
          column: originalPos.column
        },
        name: mapping.name || originalPos.name
      });
    }
  });

  original.destroy();
  humanify.destroy();

  return combined.toJSON();
}
```
