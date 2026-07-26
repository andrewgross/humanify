/**
 * The rename step's COLLISION LADDER, read back out.
 *
 * When a proposed name is taken, `src/llm/validation.ts` decorates it — first
 * with a word from `DECORATION_WORDS`, then with a numeric counter. Which
 * binding gets which decoration depends on the order the renamer happened to
 * reach them, so a decoration is a SLOT MARKER and carries no identity across
 * releases. This module strips one, so a name can be reduced to the stem the
 * ladder decorated.
 */

/** Mirrors DECORATION_WORDS in src/llm/validation.ts. */
export const DECORATION_WORDS = [
  "Val",
  "Var",
  "Ref",
  "Item",
  "Data",
  "Result",
  "Value"
];

/**
 * The name with ONE ladder decoration removed: a trailing digit run, or a
 * trailing decoration word. Trailing only and one pass only, on purpose —
 * `sha256Hasher` keeps its 256 because the digits are interior, and a stem
 * shorter than 3 characters is not a stem.
 */
export function stripLadderDecoration(name: string): string {
  const digits = name.replace(/\d+$/, "");
  if (digits !== name && digits.length >= 3) return digits;
  for (const word of DECORATION_WORDS) {
    if (name.endsWith(word)) {
      const stem = name.slice(0, -word.length);
      if (stem.length >= 3) return stem;
    }
  }
  return name;
}
