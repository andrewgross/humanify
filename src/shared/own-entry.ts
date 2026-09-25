/**
 * A record's OWN entry for `key`, or undefined. A bare `record[key]` on a
 * plain object literal falls through to Object.prototype, so an identifier
 * named `toString`, `constructor`, `hasOwnProperty`, … reads a built-in
 * function (16-findings-queue #12: prompts told a binding its prior name was
 * `function toString() { [native code] }`, and a module identifier named
 * `hasOwnProperty` crashed the prompt build). Every per-identifier record
 * lookup goes through here.
 */
export function ownEntry<T>(
  record: Record<string, T> | undefined,
  key: string
): T | undefined {
  return record !== undefined && Object.hasOwn(record, key)
    ? record[key]
    : undefined;
}
