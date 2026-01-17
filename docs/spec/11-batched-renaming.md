# Batched Renaming

## Problem

The current per-identifier renaming approach produces poor quality names because the LLM lacks semantic context:

```javascript
// Input
function a(e, t) {
  var n = [];
  var r = e.length;
  for (var i = 0; i < r; i += t) {
    n.push(e.substring(i, i + t));
  }
  return n;
}

// Current output (poor)
function aVal(eVal, tVal) {
  var nVal = [];
  var rVal = eVal.length;
  // ...
}

// Desired output
function splitStringIntoChunks(inputString, chunkSize) {
  var chunks = [];
  var stringLength = inputString.length;
  // ...
}
```

When asked "rename `e`" in isolation, the LLM doesn't understand the function's purpose.

## Solution

Ask the LLM to rename ALL identifiers in a function at once, allowing it to:
1. Understand the function semantically
2. Name related variables consistently
3. Choose a function name that reflects its purpose

### Prompt Structure

```
Analyze this minified JavaScript function and suggest descriptive names for ALL identifiers.

```javascript
function a(e, t) {
  var n = [];
  var r = e.length;
  for (var i = 0; i < r; i += t) {
    n.push(e.substring(i, i + t));
  }
  return n;
}
```

Identifiers to rename: a, e, t, n, r, i

Respond with a JSON object mapping each original name to a descriptive name:
{
  "a": "descriptiveFunctionName",
  "e": "descriptiveParamName",
  ...
}

Rules:
- Every identifier listed MUST have a mapping
- All suggested names MUST be unique (no duplicates)
- Use camelCase for variables/functions, PascalCase for classes
- Names should reflect the semantic purpose based on usage
```

### Validation & Retry Logic

The LLM response must be validated:

1. **Filter invalid mappings:**
   - Remove mappings for identifiers that don't exist
   - Remove mappings where new name equals old name
   - Remove mappings with invalid identifier syntax

2. **Detect duplicates:**
   - If multiple identifiers map to the same name, reject all duplicates
   - Add duplicates to the retry list

3. **Track missing mappings:**
   - Identifiers that weren't renamed need retry

4. **Retry with context:**
   - Regenerate function code (with partial renames applied)
   - Ask LLM to rename only the remaining identifiers
   - Include already-used names to avoid conflicts

5. **Retry limit:**
   - Maximum 3 attempts per function
   - After exhausting retries, fall back to keeping original names

### Example Flow

```
Attempt 1:
  Input identifiers: [a, e, t, n, r, i]
  LLM returns: { a: "split", e: "str", t: "size", n: "result", r: "len", i: "idx" }

  Validation:
    - All mappings valid
    - No duplicates
    - All identifiers covered

  Result: Apply all renames ✓

Attempt 1 (with issues):
  Input identifiers: [a, e, t, n, r, i]
  LLM returns: { a: "process", e: "data", t: "data", n: "result" }

  Validation:
    - "data" is duplicated → reject both e and t mappings
    - r and i are missing

  Apply valid renames: a→process, n→result

Attempt 2:
  Regenerated code with partial renames:
    function process(e, t) {
      var result = [];
      var r = e.length;
      for (var i = 0; i < r; i += t) { ... }
    }

  Input identifiers: [e, t, r, i]  // remaining
  Already used: [process, result]  // avoid these

  LLM returns: { e: "input", t: "chunkSize", r: "length", i: "index" }

  Apply remaining renames ✓
```

## Implementation

### New Types

```typescript
// src/llm/types.ts

interface BatchRenameRequest {
  /** Current function code */
  code: string;

  /** Identifiers that need renaming */
  identifiers: string[];

  /** Names already in use (must avoid) */
  usedNames: Set<string>;

  /** Callee signatures for context */
  calleeSignatures: CalleeSignature[];

  /** Call sites for context */
  callsites: string[];
}

interface BatchRenameResponse {
  /** Mapping from original name to new name */
  renames: Record<string, string>;
}
```

### New Provider Method

```typescript
// src/llm/types.ts - add to LLMProvider interface

interface LLMProvider {
  // ... existing methods

  /**
   * Suggest names for all identifiers in a function at once.
   * Returns a mapping from original name to suggested name.
   */
  suggestAllNames(request: BatchRenameRequest): Promise<BatchRenameResponse>;
}
```

### Processor Changes

```typescript
// src/rename/processor.ts

private async processFunction(fn: FunctionNode, llm: LLMProvider): Promise<void> {
  const bindings = getOwnBindings(fn.path);
  const identifiersToRename = bindings.map(b => b.name);
  const usedNames = getUsedIdentifiers(fn.path);
  const renameMapping: Record<string, string> = {};

  let remaining = new Set(identifiersToRename);
  let attempts = 0;
  const MAX_ATTEMPTS = 3;

  while (remaining.size > 0 && attempts < MAX_ATTEMPTS) {
    attempts++;

    // Get current code (with any partial renames applied)
    const code = generate(fn.path.node).code;

    // Ask LLM for batch renames
    const response = await llm.suggestAllNames({
      code,
      identifiers: [...remaining],
      usedNames,
      calleeSignatures: getCalleeSignatures(fn),
      callsites: fn.callSites.map(cs => cs.code)
    });

    // Validate and apply
    const { valid, duplicates, missing } = validateRenames(
      response.renames,
      remaining,
      usedNames
    );

    // Apply valid renames
    for (const [oldName, newName] of Object.entries(valid)) {
      fn.path.scope.rename(oldName, newName);
      usedNames.add(newName);
      renameMapping[oldName] = newName;
      remaining.delete(oldName);
    }

    // Add duplicates back to remaining for retry
    for (const name of duplicates) {
      remaining.add(name);
    }
  }

  // Any remaining identifiers keep their original names
  for (const name of remaining) {
    renameMapping[name] = name;
  }

  fn.renameMapping = { names: renameMapping };
}

function validateRenames(
  renames: Record<string, string>,
  expected: Set<string>,
  usedNames: Set<string>
): { valid: Record<string, string>; duplicates: string[]; missing: string[] } {
  const valid: Record<string, string> = {};
  const duplicates: string[] = [];
  const seenNewNames = new Set<string>();

  for (const [oldName, newName] of Object.entries(renames)) {
    // Skip if identifier doesn't exist
    if (!expected.has(oldName)) continue;

    // Skip if same as original
    if (oldName === newName) continue;

    // Skip if invalid syntax
    if (!isValidIdentifier(newName)) continue;

    // Check for duplicates (within this batch)
    if (seenNewNames.has(newName)) {
      // Find and remove the previous mapping that used this name
      for (const [k, v] of Object.entries(valid)) {
        if (v === newName) {
          delete valid[k];
          duplicates.push(k);
          break;
        }
      }
      duplicates.push(oldName);
      continue;
    }

    // Check for conflict with existing names
    if (usedNames.has(newName)) {
      duplicates.push(oldName);
      continue;
    }

    valid[oldName] = newName;
    seenNewNames.add(newName);
  }

  // Find missing identifiers
  const missing = [...expected].filter(
    name => !valid[name] && !duplicates.includes(name)
  );

  return { valid, duplicates, missing };
}
```

## Benefits

1. **Better semantic understanding** - LLM sees full function context
2. **Consistent naming** - Related variables named together
3. **Fewer LLM calls** - 1-3 calls per function vs N calls per identifier
4. **Still safe** - Uses Babel's `scope.rename()`, no code rewriting

## Migration

This is a new approach that replaces the per-identifier renaming. The old `suggestName` method can be kept for backward compatibility but `suggestAllNames` will be preferred when available.
