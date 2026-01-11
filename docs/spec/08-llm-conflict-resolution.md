# LLM-Based Conflict Resolution

## Problem

Current approach when LLM suggests a name that's already in use:

```typescript
// Algorithmic fallback - produces generic names
while (context.usedIdentifiers.has(newName)) {
  newName = resolveConflict(newName, context.usedIdentifiers);
}
// "user" → "userVal" → "userVar" → "user2" ...
```

This loses semantic meaning. If the LLM wanted to call something `user` but that's taken, `userVal` doesn't capture the intent - maybe `currentUser`, `targetUser`, or `fetchedUser` would be better.

## Solution

Re-prompt the LLM with feedback when a conflict occurs:

```
Your previous suggestion "user" is already in use in this scope.
Please suggest a different name. Already used: user, data, result, ...
```

The LLM can then pick a semantically appropriate alternative based on context.

## Implementation

### 1. Update prompts.ts

Add retry prompt builder:

```typescript
export function buildRetryPrompt(
  currentName: string,
  rejectedName: string,
  context: LLMContext,
  reason: string
): string {
  let prompt = `Your previous suggestion "${rejectedName}" cannot be used: ${reason}\n\n`;
  prompt += `Please suggest a different name for "${currentName}" in this code:\n\n`;
  prompt += "```javascript\n" + context.functionCode + "\n```\n\n";

  const usedList = [...context.usedIdentifiers].slice(0, 50).join(", ");
  prompt += `Names already in use (you must avoid ALL of these): ${usedList}\n`;

  return prompt;
}
```

Update `buildFunctionNamePrompt` to include used identifiers (currently missing).

### 2. Update LLMProvider interface

Add optional retry method:

```typescript
export interface LLMProvider {
  suggestName(currentName: string, context: LLMContext): Promise<NameSuggestion>;

  // Retry with feedback about why previous suggestion was rejected
  retrySuggestName?(
    currentName: string,
    rejectedName: string,
    reason: string,
    context: LLMContext
  ): Promise<NameSuggestion>;
}
```

### 3. Update processor.ts

Replace algorithmic resolution with LLM retry loop:

```typescript
const MAX_NAME_RETRIES = 9;

async processBinding(binding: Binding, context: LLMContext): Promise<string> {
  let suggestion = await this.llm.suggestName(binding.name, context);
  let newName = sanitizeIdentifier(suggestion.name);
  let attempts = 0;

  while (context.usedIdentifiers.has(newName) && attempts < MAX_NAME_RETRIES) {
    attempts++;

    if (this.llm.retrySuggestName) {
      suggestion = await this.llm.retrySuggestName(
        binding.name,
        newName,
        `"${newName}" is already in use`,
        context
      );
    } else {
      // Fallback for providers without retry support
      suggestion = await this.llm.suggestName(binding.name, {
        ...context,
        // Add rejected name to used set to discourage it
        usedIdentifiers: new Set([...context.usedIdentifiers, newName])
      });
    }

    newName = sanitizeIdentifier(suggestion.name);
  }

  // Final fallback if LLM keeps suggesting conflicts
  if (context.usedIdentifiers.has(newName)) {
    newName = resolveConflict(newName, context.usedIdentifiers);
  }

  return newName;
}
```

### 4. Update OpenAICompatibleProvider

Implement retry with conversation history:

```typescript
async retrySuggestName(
  currentName: string,
  rejectedName: string,
  reason: string,
  context: LLMContext
): Promise<NameSuggestion> {
  const response = await this.client.chat.completions.create({
    model: this.model,
    messages: [
      { role: "system", content: SYSTEM_PROMPT },
      { role: "user", content: buildUserPrompt(currentName, context) },
      { role: "assistant", content: JSON.stringify({ name: rejectedName }) },
      { role: "user", content: buildRetryPrompt(currentName, rejectedName, context, reason) }
    ],
    response_format: { type: "json_object" },
    temperature: this.temperature,
    max_tokens: this.maxTokens
  });

  // ... parse response
}
```

## Retry Reasons

The retry mechanism can handle multiple rejection reasons:

| Reason | Message |
|--------|---------|
| Name in use | `"user" is already in use in this scope` |
| Reserved word | `"class" is a JavaScript reserved word` |
| Invalid identifier | `"123abc" is not a valid JavaScript identifier` |
| Too long | `Name exceeds 50 character limit` |

## Configuration

```typescript
interface ProcessorOptions {
  maxNameRetries?: number;  // Default: 9
  // ... other options
}
```

## Metrics

Track retry statistics:

```typescript
interface LLMMetrics {
  // ... existing
  retryCount: number;      // Total retries across all names
  retriesSucceeded: number; // Retries that found valid name
  retriesFailed: number;    // Fell back to algorithmic resolution
}
```

## Example Flow

```
1. LLM suggests: "user"
   → Rejected: already in use

2. Retry prompt: "user" is already in use, suggest different name
   LLM suggests: "currentUser"
   → Rejected: already in use

3. Retry prompt: "currentUser" is already in use, suggest different name
   LLM suggests: "activeUser"
   → Accepted!
```

vs current approach:
```
1. LLM suggests: "user"
   → Conflict resolved to: "userVal"  (loses semantic meaning)
```

## Backwards Compatibility

- `retrySuggestName` is optional on `LLMProvider`
- If not implemented, falls back to re-calling `suggestName` with updated context
- Algorithmic `resolveConflict` remains as final fallback after MAX_NAME_RETRIES (9)

## Cost Consideration

Each retry is an additional LLM call. With MAX_NAME_RETRIES=9, worst case is 10x the calls for a single identifier. In practice:
- Most names won't conflict (used identifiers list helps)
- Conflicts are rare after first rename in a scope
- Better names justify the cost

To mitigate:
- Track retry rate in metrics
- If retry rate is high, may indicate prompt issues
- Could add option to disable retries for cost-sensitive users
