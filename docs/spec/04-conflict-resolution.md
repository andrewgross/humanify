# Conflict Resolution

## The Problem

The current implementation resolves naming conflicts by prepending underscores:

```javascript
// Current behavior (bad)
const data = fetch();
const _data = process();
const __data = transform();
const ___data = finalize();  // Unreadable!
```

This produces code that isn't human-readable, defeating the purpose.

## Goals

1. Avoid underscore spam
2. Produce semantically meaningful alternative names
3. Maintain valid JavaScript identifiers
4. Respect scope rules

## Resolution Strategies

### Strategy 1: Semantic Suffixes

Add context-based suffixes that clarify the variable's role:

```typescript
const SEMANTIC_SUFFIXES = [
  // Type-based
  'Data', 'Value', 'Result', 'Item', 'Entry',
  // Role-based
  'Input', 'Output', 'Temp', 'Current', 'New', 'Old',
  // Index-based (last resort)
  '2', '3', '4'
];

function addSemanticSuffix(
  baseName: string,
  context: ConflictContext
): string {
  // Try to infer appropriate suffix from usage
  if (context.isParameter) {
    return baseName + 'Param';
  }
  if (context.isReturnValue) {
    return baseName + 'Result';
  }
  if (context.isLoopVariable) {
    return baseName + 'Item';
  }

  // Generic suffixes
  for (const suffix of SEMANTIC_SUFFIXES) {
    const candidate = baseName + suffix;
    if (!context.usedNames.has(candidate)) {
      return candidate;
    }
  }

  // Numeric fallback
  let i = 2;
  while (context.usedNames.has(`${baseName}${i}`)) {
    i++;
  }
  return `${baseName}${i}`;
}
```

### Strategy 2: Synonyms

Use a thesaurus of common programming terms:

```typescript
const SYNONYMS: Record<string, string[]> = {
  'data': ['info', 'payload', 'content', 'record'],
  'result': ['output', 'response', 'outcome', 'value'],
  'count': ['total', 'num', 'quantity', 'amount'],
  'index': ['idx', 'pos', 'position', 'offset'],
  'item': ['element', 'entry', 'record', 'node'],
  'user': ['account', 'member', 'person', 'profile'],
  'error': ['err', 'exception', 'failure', 'issue'],
  'callback': ['handler', 'listener', 'fn', 'hook'],
  'config': ['options', 'settings', 'params', 'props'],
  'temp': ['tmp', 'scratch', 'working', 'interim'],
  'list': ['array', 'items', 'collection', 'set'],
  'map': ['dict', 'lookup', 'index', 'table'],
  'key': ['id', 'name', 'identifier', 'ref'],
  'value': ['val', 'data', 'content', 'payload'],
  'start': ['begin', 'first', 'head', 'from'],
  'end': ['finish', 'last', 'tail', 'to'],
  'current': ['curr', 'active', 'present', 'now'],
  'previous': ['prev', 'last', 'prior', 'old'],
  'next': ['following', 'upcoming', 'subsequent'],
};

function findSynonym(
  baseName: string,
  usedNames: Set<string>
): string | null {
  const synonyms = SYNONYMS[baseName.toLowerCase()];
  if (!synonyms) return null;

  for (const synonym of synonyms) {
    // Preserve original casing style
    const candidate = matchCase(synonym, baseName);
    if (!usedNames.has(candidate)) {
      return candidate;
    }
  }
  return null;
}
```

### Strategy 3: Scope-Aware Prefixes

Instead of generic `_`, use meaningful prefixes based on scope:

```typescript
function getScopePrefix(path: NodePath): string {
  const parent = path.parentPath;

  if (parent?.isForStatement()) return 'loop';
  if (parent?.isCatchClause()) return 'caught';
  if (parent?.isArrowFunctionExpression()) return 'inner';
  if (parent?.isClassMethod()) return 'method';

  // Check containing function name
  const containingFn = path.getFunctionParent();
  if (containingFn?.node.id?.name) {
    // e.g., inside fetchUser -> "fetchUserData" instead of "_data"
    return containingFn.node.id.name;
  }

  return 'local';
}
```

### Strategy 4: LLM Fallback

Ask the LLM for an alternative when simple strategies fail:

```typescript
async function askLLMForAlternative(
  originalSuggestion: string,
  context: LLMContext,
  llm: LLMProvider
): Promise<string> {
  const prompt = `
The name "${originalSuggestion}" is already in use in this scope.

Existing names: ${[...context.usedIdentifiers].join(', ')}

Suggest an alternative name that:
1. Conveys the same meaning
2. Doesn't conflict with existing names
3. Follows JavaScript naming conventions

Code context:
${context.functionCode}

Respond with JSON: { "name": "alternativeName" }
`;

  const response = await llm.complete(prompt);
  return JSON.parse(response).name;
}
```

## Resolution Pipeline

```typescript
// src/rename/conflict-resolver.ts

interface ConflictContext {
  proposedName: string;
  usedNames: Set<string>;
  binding: Binding;
  scope: Scope;
  llm?: LLMProvider;
}

class ConflictResolver {
  async resolve(ctx: ConflictContext): Promise<string> {
    const { proposedName, usedNames } = ctx;

    // No conflict - use as-is
    if (!this.hasConflict(proposedName, ctx)) {
      return proposedName;
    }

    // Strategy 1: Try synonyms
    const synonym = findSynonym(proposedName, usedNames);
    if (synonym && !this.hasConflict(synonym, ctx)) {
      return synonym;
    }

    // Strategy 2: Add semantic suffix
    const suffixed = addSemanticSuffix(proposedName, ctx);
    if (!this.hasConflict(suffixed, ctx)) {
      return suffixed;
    }

    // Strategy 3: Scope-aware prefix
    const prefix = getScopePrefix(ctx.binding.path);
    const prefixed = prefix + capitalize(proposedName);
    if (!this.hasConflict(prefixed, ctx)) {
      return prefixed;
    }

    // Strategy 4: Ask LLM (if available and worth the cost)
    if (ctx.llm && this.worthAskingLLM(proposedName)) {
      try {
        const alternative = await askLLMForAlternative(
          proposedName,
          buildLLMContext(ctx),
          ctx.llm
        );
        if (!this.hasConflict(alternative, ctx)) {
          return alternative;
        }
      } catch {
        // LLM failed, continue to fallback
      }
    }

    // Strategy 5: Numeric suffix (last resort, but better than ___)
    let i = 2;
    while (this.hasConflict(`${proposedName}${i}`, ctx)) {
      i++;
    }
    return `${proposedName}${i}`;
  }

  private hasConflict(name: string, ctx: ConflictContext): boolean {
    // Check if name is in use
    if (ctx.usedNames.has(name)) return true;

    // Check if it's a reserved word
    if (RESERVED_WORDS.has(name)) return true;

    // Check if scope has this binding
    if (ctx.scope.hasBinding(name)) return true;

    // Check if it shadows something important
    if (this.wouldShadowImportant(name, ctx.scope)) return true;

    return false;
  }

  private wouldShadowImportant(name: string, scope: Scope): boolean {
    // Check parent scopes for important bindings
    let current = scope.parent;
    while (current) {
      const binding = current.getOwnBinding(name);
      if (binding) {
        // Allow shadowing if parent binding is also minified
        if (isMinifiedName(binding.identifier.name)) {
          return false;
        }
        // Don't shadow meaningful names from parent scope
        return true;
      }
      current = current.parent;
    }
    return false;
  }

  private worthAskingLLM(name: string): boolean {
    // Don't bother LLM for short/generic names
    return name.length >= 4 && !GENERIC_NAMES.has(name);
  }
}
```

## Reserved Words and Builtins

```typescript
const RESERVED_WORDS = new Set([
  // JavaScript reserved
  'break', 'case', 'catch', 'continue', 'debugger', 'default', 'delete',
  'do', 'else', 'finally', 'for', 'function', 'if', 'in', 'instanceof',
  'new', 'return', 'switch', 'this', 'throw', 'try', 'typeof', 'var',
  'void', 'while', 'with', 'class', 'const', 'enum', 'export', 'extends',
  'import', 'super', 'implements', 'interface', 'let', 'package', 'private',
  'protected', 'public', 'static', 'yield', 'await', 'async',

  // Common globals (avoid shadowing)
  'undefined', 'null', 'true', 'false', 'NaN', 'Infinity',
  'console', 'window', 'document', 'global', 'process',
  'Array', 'Object', 'String', 'Number', 'Boolean', 'Symbol',
  'Map', 'Set', 'WeakMap', 'WeakSet', 'Promise', 'Proxy',
  'Error', 'TypeError', 'RangeError', 'SyntaxError',
  'JSON', 'Math', 'Date', 'RegExp', 'Function',
]);

const GENERIC_NAMES = new Set([
  'a', 'b', 'c', 'x', 'y', 'z', 'i', 'j', 'k', 'n', 'm',
  'tmp', 'temp', 'foo', 'bar', 'baz', 'qux',
]);
```

## Examples

### Before (current behavior)

```javascript
function process(data) {
  const _data = transform(data);
  const __data = validate(_data);
  const ___data = format(__data);
  return ___data;
}
```

### After (new behavior)

```javascript
function process(data) {
  const transformedData = transform(data);
  const validatedData = validate(transformedData);
  const formattedData = format(validatedData);
  return formattedData;
}
```

Or with synonyms:

```javascript
function process(data) {
  const payload = transform(data);
  const validated = validate(payload);
  const output = format(validated);
  return output;
}
```

## Integration with Rename Pipeline

```typescript
async function processFunction(fn: FunctionNode, llm: LLMProvider) {
  const resolver = new ConflictResolver();
  const context = extractContext(fn);
  const usedNames = new Set(context.usedIdentifiers);

  for (const binding of getOwnBindings(fn.path)) {
    // Get LLM suggestion
    const suggestion = await llm.suggestName(binding.name, context);

    // Resolve any conflicts
    const finalName = await resolver.resolve({
      proposedName: suggestion.name,
      usedNames,
      binding,
      scope: fn.path.scope,
      llm  // For fallback queries
    });

    // Track and apply
    this.trackRename(binding, finalName);
    fn.path.scope.rename(binding.name, finalName);

    // Update used names for next iteration
    usedNames.add(finalName);
  }
}
```
