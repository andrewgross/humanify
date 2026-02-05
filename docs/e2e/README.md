# E2E Fingerprint Validation Harness

The E2E validation harness tests whether humanify's fingerprinting system correctly identifies functions across different versions of minified code.

## Purpose

When humanify processes minified JavaScript, it builds fingerprints for each function. These fingerprints enable:

1. **Cache reuse**: If you've already processed a function and it hasn't changed, reuse the cached humanified names
2. **Cross-version matching**: When processing v2 of a codebase, identify which functions are the same as v1

The E2E harness validates that fingerprinting works correctly by:

1. Taking two versions of real open-source packages (e.g., mitt v3.0.0 and v3.0.1)
2. Minifying both versions ourselves (so we control the process)
3. Computing "ground truth" by diffing the original source ASTs
4. Running humanify's fingerprinting on the minified outputs
5. Comparing fingerprint matching results against ground truth

## Key Metrics

- **Cache Reuse Accuracy**: When a function is unchanged between versions, do fingerprints correctly match? (Should be 100%)
- **Change Detection Accuracy**: When a function is modified, do fingerprints correctly differ? (Should be high)
- **False Match Rate**: Are new functions incorrectly matched to old ones? (Should be 0%)

## Quick Start

```bash
# List available test fixtures
npm run e2e -- list

# Set up a fixture (downloads source, compiles, etc.)
npm run e2e -- setup mitt

# Run validation
npm run e2e -- validate mitt

# Run with verbose output
npm run e2e -- validate mitt --verbose

# Update CI snapshot baseline
npm run e2e -- validate mitt --update-snapshot

# CI mode (compare against snapshot, fail on drift)
npm run e2e -- validate mitt --ci

# Debug a specific function
npm run e2e -- debug mitt 3.0.0 3.0.1 --function emit
```

## Directory Structure

```
test/e2e/
├── harness/                    # The validation harness code
│   ├── index.ts                # CLI entry point
│   ├── setup.ts                # Download/prepare fixtures
│   ├── minify.ts               # Minification with source maps
│   ├── ground-truth.ts         # AST diff for ground truth
│   ├── validate.ts             # Run fingerprinting + compare
│   ├── reporter.ts             # Output results
│   ├── debug.ts                # Debug artifact generation
│   ├── snapshot.ts             # CI snapshot comparison
│   └── code-extractor.ts       # Extract code snippets
├── fixtures/                   # Test packages
│   └── mitt/
│       ├── fixture.config.json # Package configuration
│       ├── source/             # Original source (gitignored)
│       ├── build/              # Compiled JS (gitignored)
│       └── minified/           # Minified output (gitignored)
├── snapshots/                  # CI baselines (committed)
│   └── mitt/
│       └── v3.0.0-v3.0.1-terser-default.snapshot.json
└── output/                     # Debug artifacts (gitignored)
```

## Current Test Fixtures

| Package | Versions | Description |
|---------|----------|-------------|
| mitt    | 3.0.0 → 3.0.1 | Tiny event emitter (~100 LOC) |

## Further Reading

- [CLI Commands Reference](./commands.md)
- [Adding New Fixtures](./adding-fixtures.md)
- [Spec: E2E Validation Harness](../spec/13-e2e-validation-harness.md)
- [Spec: Cross-Version Fingerprinting](../spec/12-cross-version-fingerprinting.md)
