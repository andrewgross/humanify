/**
 * Library prefix resolver: deterministically renames identifiers
 * in library functions by prefixing with the sanitized library name.
 *
 * e.g., for react-dom: Xuo -> react_dom_Xuo
 */

/**
 * Convert a library name to a valid snake_case prefix.
 *
 * - Strips @ scope prefix
 * - Converts /, -, . to underscores
 * - Lowercases everything
 * - Prepends underscore if result starts with a digit
 */
export function sanitizeLibraryName(name: string): string {
  let result = name
    .replace(/^@/, "")
    .replace(/[/\-.]/g, "_")
    .toLowerCase();

  // Ensure the result starts with a letter or underscore
  if (/^\d/.test(result)) {
    result = `_${result}`;
  }

  return result;
}

/**
 * Resolver that prefixes identifiers with a library name.
 */
export class LibraryPrefixResolver {
  readonly strategy = "library-prefix" as const;

  constructor(private prefix: string) {}

  /**
   * Returns a mapping of oldName -> prefixedName for each identifier.
   */
  resolveNames(identifiers: string[]): Record<string, string> {
    const names: Record<string, string> = {};
    for (const id of identifiers) {
      names[id] = `${this.prefix}_${id}`;
    }
    return names;
  }
}
