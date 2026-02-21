# Spec 17: Block-Level Chunking for Giant Wrapper Bodies

**Status**: Future / Not yet implemented

## Problem

After detecting a giant IIFE wrapper and renaming its module-level bindings
(Phase 1), some variables inside the wrapper body remain unrenamed. These are
variables that only appear inside control flow blocks (if/for/while/switch) and
aren't visible at the declaration level. The wrapper itself is too large to send
to the LLM as a single function.

## Proposed Solution

### Block extraction as pseudo-functions

For wrapper bodies that exceed a size threshold (e.g., 200 lines), extract
top-level blocks (if, for, while, switch, try) as pseudo-FunctionNodes:

1. Walk the wrapper's body statements
2. For each compound statement (if/for/while/switch/try), create a
   pseudo-FunctionNode with:
   - The block's code as `functionCode`
   - Bindings declared inside the block as identifiers to rename
   - Parent scope declarations as read-only context
3. Process these pseudo-functions through the normal rename pipeline

### Sliding window for loose variables

Variables declared at the wrapper's top level but used only in distant
locations need a different approach:

1. Group declarations by their position in the body
2. For each group, collect a "window" of surrounding statements (e.g., 50 lines
   before and after)
3. Send the windowed code + declarations to the LLM as a mini rename request

### Ordering

- Block pseudo-functions should be processed after Phase 1 (module-level
  rename) but before Phase 2 (inner function processing)
- Sliding window passes run after block extraction

## Considerations

- Pseudo-FunctionNodes shouldn't appear in the function graph or dependency
  tracking — they're a Phase 1.5 concept
- Need to ensure renames from block extraction don't conflict with subsequent
  inner function renames
- The wrapper's scope should be used for conflict checking, not individual
  block scopes

## Dependencies

- Requires wrapper IIFE detection (implemented in Change 2)
- Requires parent context vars in prompts (implemented in Change 3)
