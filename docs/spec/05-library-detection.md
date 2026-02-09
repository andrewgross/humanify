# Library Detection

## Overview

Minified bundles often contain library code (React, lodash, etc.) alongside application code. Processing library code is wasteful:

1. **No benefit** - Library code is already well-documented; users can reference official docs
2. **Cost** - Library code can be 80%+ of a bundle; processing it wastes LLM calls
3. **Worse results** - LLM might rename `useState` to something confusing

## Goals

1. Identify known libraries in unpacked bundles
2. Mark library functions as "external" (skip processing)
3. Focus LLM resources on novel application code

## Detection Strategies

### Strategy 1: Module Path Matching

After webcrack unpacks, check module paths:

```typescript
// Webcrack often preserves original module structure
const LIBRARY_PATHS = [
  /node_modules\//,
  /^react(-dom)?$/,
  /^lodash/,
  /^@babel\/runtime/,
  /^core-js/,
  /^regenerator-runtime/,
  /^tslib/,
];

function isLibraryPath(modulePath: string): boolean {
  return LIBRARY_PATHS.some(pattern => pattern.test(modulePath));
}
```

### Strategy 2: Signature Matching

Identify libraries by characteristic code patterns:

```typescript
interface LibrarySignature {
  name: string;
  weight: number;  // Higher = more reliable indicator

  // Patterns to match in code
  patterns: Array<{
    regex: RegExp;
    weight: number;
  }>;

  // Known export names
  exports: string[];
}

const LIBRARY_SIGNATURES: LibrarySignature[] = [
  {
    name: 'react',
    weight: 10,
    patterns: [
      { regex: /\.__SECRET_INTERNALS_DO_NOT_USE_OR_YOU_WILL_BE_FIRED/, weight: 10 },
      { regex: /\.createElement\s*\(/, weight: 3 },
      { regex: /\bReact(?:DOM)?\b/, weight: 5 },
      { regex: /use(?:State|Effect|Memo|Callback|Ref|Context)\s*\(/, weight: 2 },
    ],
    exports: ['createElement', 'useState', 'useEffect', 'Component', 'Fragment'],
  },
  {
    name: 'lodash',
    weight: 8,
    patterns: [
      { regex: /\b_\.(?:map|filter|reduce|forEach|find)\b/, weight: 5 },
      { regex: /lodash/, weight: 10 },
    ],
    exports: ['map', 'filter', 'reduce', 'debounce', 'throttle', 'cloneDeep'],
  },
  {
    name: 'moment',
    weight: 8,
    patterns: [
      { regex: /\.(?:format|startOf|endOf|add|subtract)\s*\(['"]\w+['"]\)/, weight: 3 },
      { regex: /moment(?:\.tz)?/, weight: 10 },
    ],
    exports: ['moment', 'duration', 'utc'],
  },
  {
    name: 'axios',
    weight: 7,
    patterns: [
      { regex: /axios\.(?:get|post|put|delete|request)/, weight: 8 },
      { regex: /\.interceptors\.(?:request|response)/, weight: 6 },
    ],
    exports: ['axios', 'create', 'interceptors'],
  },
  {
    name: 'redux',
    weight: 8,
    patterns: [
      { regex: /createStore\s*\(/, weight: 5 },
      { regex: /combineReducers\s*\(/, weight: 7 },
      { regex: /\.dispatch\s*\(\s*\{/, weight: 3 },
    ],
    exports: ['createStore', 'combineReducers', 'applyMiddleware', 'bindActionCreators'],
  },
  // Add more libraries as needed
];

function detectLibrary(code: string): { name: string; confidence: number } | null {
  for (const sig of LIBRARY_SIGNATURES) {
    let score = 0;
    for (const pattern of sig.patterns) {
      if (pattern.regex.test(code)) {
        score += pattern.weight;
      }
    }

    const confidence = score / sig.weight;
    if (confidence >= 0.5) {
      return { name: sig.name, confidence };
    }
  }
  return null;
}
```

### Strategy 3: Bundle Comment Detection

Many bundlers preserve library info in comments:

```typescript
const BUNDLE_COMMENTS = [
  // Webpack
  /\/\*!\s*(\S+)\s+v[\d.]+/,           // /*! react v18.2.0 */
  /\/\*\*\s*@license\s+(\S+)/,          // /** @license React */

  // Rollup
  /\/\*\*\s*\*\s*@module\s+(\S+)/,      // /** * @module lodash */

  // UMD wrappers
  /typeof exports\s*===?\s*['"]object['"]/,
];

function extractLibraryFromComments(code: string): string[] {
  const libraries: string[] = [];

  for (const pattern of BUNDLE_COMMENTS) {
    const matches = code.matchAll(new RegExp(pattern, 'g'));
    for (const match of matches) {
      if (match[1]) {
        libraries.push(match[1].toLowerCase());
      }
    }
  }

  return libraries;
}
```

### Strategy 4: Known Minified Patterns

Some minifiers produce recognizable patterns for specific libraries:

```typescript
// React's production build has characteristic patterns
const REACT_PROD_PATTERNS = [
  // React's scheduler
  /function\s+\w+\(\w+,\w+,\w+,\w+,\w+\)\{[\s\S]{0,50}priorityLevel/,
  // React's reconciler
  /beginWork|completeWork|commitRoot/,
];

// Detect by structural patterns even when fully minified
function detectMinifiedLibrary(ast: t.File): string | null {
  // Count characteristic node patterns
  const patterns = {
    reactHooks: 0,
    reduxActions: 0,
    // etc.
  };

  traverse(ast, {
    CallExpression(path) {
      // React hooks pattern: useXxx() where Xxx is capitalized
      if (t.isIdentifier(path.node.callee) &&
          /^use[A-Z]/.test(path.node.callee.name)) {
        patterns.reactHooks++;
      }
    }
  });

  // Threshold-based detection
  if (patterns.reactHooks > 10) return 'react';

  return null;
}
```

## Classification Pipeline

```typescript
// src/analysis/library-detector.ts

interface ClassificationResult {
  libraries: Map<string, string[]>;  // library name -> file paths
  novel: string[];                    // files with application code
  mixed: string[];                    // files with both (need selective processing)
}

class LibraryDetector {
  async classify(files: UnpackedFile[]): Promise<ClassificationResult> {
    const result: ClassificationResult = {
      libraries: new Map(),
      novel: [],
      mixed: [],
    };

    for (const file of files) {
      const detection = this.detectLibraries(file);

      if (detection.isFullyLibrary) {
        // Entire file is library code
        const existing = result.libraries.get(detection.libraryName!) || [];
        existing.push(file.path);
        result.libraries.set(detection.libraryName!, existing);
      } else if (detection.hasLibraryCode) {
        // Mixed file - needs function-level classification
        result.mixed.push(file.path);
      } else {
        // Novel application code
        result.novel.push(file.path);
      }
    }

    return result;
  }

  private detectLibraries(file: UnpackedFile): {
    isFullyLibrary: boolean;
    hasLibraryCode: boolean;
    libraryName?: string;
  } {
    // Try path-based detection first (fast)
    if (isLibraryPath(file.path)) {
      return {
        isFullyLibrary: true,
        hasLibraryCode: true,
        libraryName: extractLibraryName(file.path),
      };
    }

    // Try comment detection
    const fromComments = extractLibraryFromComments(file.code);
    if (fromComments.length > 0) {
      return {
        isFullyLibrary: true,
        hasLibraryCode: true,
        libraryName: fromComments[0],
      };
    }

    // Try signature detection
    const signature = detectLibrary(file.code);
    if (signature && signature.confidence > 0.8) {
      return {
        isFullyLibrary: true,
        hasLibraryCode: true,
        libraryName: signature.name,
      };
    }

    // No library detected
    return {
      isFullyLibrary: false,
      hasLibraryCode: signature !== null,
    };
  }
}
```

## Function-Level Classification

For mixed files, classify individual functions:

```typescript
function classifyFunction(fn: FunctionNode): 'library' | 'novel' | 'unknown' {
  const code = generate(fn.path.node).code;

  // Check against library signatures
  const detection = detectLibrary(code);
  if (detection && detection.confidence > 0.7) {
    return 'library';
  }

  // Heuristics for novel code
  const novelIndicators = [
    // Application-specific naming (if not fully minified)
    /fetch(?:User|Product|Order)/i,
    /handle(?:Click|Submit|Change)/i,
    /validate(?:Form|Input|Email)/i,
  ];

  for (const pattern of novelIndicators) {
    if (pattern.test(code)) {
      return 'novel';
    }
  }

  // If heavily minified, assume novel (libraries usually have some artifacts)
  const minificationScore = getMinificationScore(code);
  if (minificationScore > 0.8) {
    return 'novel';  // Probably application code that was minified
  }

  return 'unknown';
}

function getMinificationScore(code: string): number {
  // Higher score = more minified
  const factors = [
    code.split('\n').length < 5,           // Few lines
    /^[a-z]$/.test(extractIdentifiers(code)[0] || ''),  // Single-letter vars
    code.length / code.split('\n').length > 200,  // Long lines
  ];

  return factors.filter(Boolean).length / factors.length;
}
```

## Integration with Pipeline

```typescript
// In pipeline context
interface PipelineContext {
  // After classification
  libraryFunctions: Set<string>;  // sessionIds to skip
  novelFunctions: FunctionNode[]; // Functions to process
}

// During ready queue initialization
function initializeWorkQueue(
  allFunctions: FunctionNode[],
  libraryFunctions: Set<string>
): WorkQueue {
  const queue = new WorkQueue();

  for (const fn of allFunctions) {
    if (libraryFunctions.has(fn.sessionId)) {
      // Mark as "done" immediately - don't process
      queue.markAsLibrary(fn);
    } else {
      queue.add(fn);
    }
  }

  return queue;
}
```

## CLI Options

```bash
# Auto-detect and skip libraries (default)
humanify bundle.min.js -o output/

# Force processing of everything (including libraries)
humanify bundle.min.js --no-skip-libraries -o output/

# Only process specific patterns
humanify bundle.min.js --include "src/**" --exclude "vendor/**" -o output/

# Show what would be skipped
humanify bundle.min.js --dry-run -v
# Output:
# Detected libraries:
#   react (3 files, 1,247 functions)
#   lodash (1 file, 412 functions)
# Novel code:
#   12 files, 89 functions
# Would process: 89 functions (skipping 1,659 library functions)
```

## Known Libraries Database

Maintain a database of known libraries with their signatures:

```typescript
// Could be loaded from external file for easy updates
const KNOWN_LIBRARIES_DB = {
  version: 1,
  libraries: [
    {
      name: 'react',
      npmPackage: 'react',
      signatures: { /* ... */ },
      knownHashes: [
        // Structural hashes of known React internals
        'a1b2c3d4...',
        'e5f6g7h8...',
      ]
    },
    // ...
  ]
};

// Use known hashes to instantly identify library functions
function isKnownLibraryFunction(hash: string): string | null {
  for (const lib of KNOWN_LIBRARIES_DB.libraries) {
    if (lib.knownHashes.includes(hash)) {
      return lib.name;
    }
  }
  return null;
}
```

This allows community contribution of library signatures without code changes.
