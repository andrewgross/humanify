# Spec 18: Improved Minified Name Detection

**Status**: Future / Not yet implemented

## Problem

The current `looksMinified()` heuristic uses a combination of:
- Length-based rules (1-2 chars almost always minified, 5+ never)
- Allowlists for common short names (2-char and 3-char)
- Pattern matching for unusual casing and digits (3-4 char)

This works well for most bundlers but has limitations:
- The 3-char allowlist is manually curated and may miss domain-specific names
- 5+ char names are never flagged, but some minifiers produce longer names
- No consideration of the file's overall naming patterns

## Proposed Approaches

### 1. Statistical analysis

Instead of per-name heuristics, analyze the entire file's identifier
distribution:

- If >70% of names are 1-3 chars, the file is likely minified → be aggressive
- If most names are descriptive with a few short ones, be conservative
- This adapts to the specific bundle's style

### 2. Entropy-based detection

Minified names tend to have higher character entropy (random-looking
combinations) compared to real names:

- `fetchUserData` has low entropy — common English character patterns
- `xRTdE` has high entropy — unusual character combinations
- Could use bigram/trigram frequency tables from English

### 3. Tracking-based approach

Track which names the bundler's name mangler produces by analyzing the
module boundary patterns:

- Webpack: `__webpack_require__`, `__webpack_exports__` → all short locals
  are mangled
- Bun: `__require`, `__commonJS` → similar
- If we detect a bundler, apply that bundler's known mangling patterns

### 4. AST-aware detection

Look at how names are used, not just their form:

- A 4-char name used as a parameter in 50 different functions → likely
  minified (same mangled name reused)
- A 3-char name always used with `.prototype` → likely a class name,
  possibly mangled
- A name that shadows a well-known global → likely mangled

## Current Implementation

The current `looksMinified()` in `src/rename/minified-heuristic.ts` implements
approach 1 partially (via length buckets and pattern matching). Future work
would layer on statistical analysis and tracking-based detection.

## Risks

- False positives on real short names waste LLM calls and may produce
  worse names than the originals
- False negatives leave minified names untouched, reducing output quality
- Any approach needs to be fast — called once per binding, potentially
  thousands of times per file
