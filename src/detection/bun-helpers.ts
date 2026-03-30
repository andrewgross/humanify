/**
 * Structural identification of Bun's CJS helper functions.
 *
 * Bun minifies all helper names between builds, so detection is
 * purely structural — matching function body shapes, not names.
 *
 * Shared by the unpack adapter (pre-rename) and split module
 * detection (post-rename).
 */

export interface IdentifiedHelper {
  name: string;
  startOffset: number;
}

/**
 * Identify the CJS factory helper by scanning for a variable binding
 * whose body contains `{exports:{}}`.
 *
 * Bun pattern: `var x = (I, A) => () => (A || I((A = {exports: {}}).exports, A), A.exports);`
 * The `{exports:{}}` shape is always present regardless of minified names.
 *
 * In real Bun output, the factory is often part of a comma-separated var
 * declaration (e.g., `var ...other..., x=(A,q)=>()=>...`), so we match
 * `IDENT=` immediately before the arrow function rather than requiring
 * `var IDENT =`.
 */
export function identifyBunCjsFactory(source: string): IdentifiedHelper | null {
  const factoryBodyRe = /\{exports:\s*\{\}\}/g;

  for (
    let match = factoryBodyRe.exec(source);
    match !== null;
    match = factoryBodyRe.exec(source)
  ) {
    // Walk backwards to find `IDENT=` (either after `var ` or after `,`)
    const before = source.slice(0, match.index);
    // Match the closest preceding binding: `NAME=` preceded by var/let/const/comma
    const bindingMatch = before.match(
      /(?:(?:var|let|const)\s+|,)(\w+)\s*=\s*[^;]*$/
    );
    if (bindingMatch) {
      return {
        name: bindingMatch[1],
        startOffset: before.length - bindingMatch[0].length
      };
    }
  }

  return null;
}

/**
 * Identify the require variable by tracing the createRequire import.
 *
 * Bun pattern:
 *   `import{createRequire as Glq}from"node:module";`
 *   `var m6 = Glq(import.meta.url);`
 *
 * Returns the require variable name (e.g., "m6") or null.
 */
export function identifyBunRequire(source: string): string | null {
  // Step 1: Find the createRequire alias
  const importMatch = source.match(
    /import\s*\{[^}]*createRequire\s+as\s+(\w+)[^}]*\}\s*from\s*["']node:module["']/
  );
  if (!importMatch) return null;

  const alias = importMatch[1];

  // Step 2: Find `var NAME = ALIAS(import.meta.url)`
  const varRe = new RegExp(
    `(?:var|let|const)\\s+(\\w+)\\s*=\\s*${escapeRegExp(alias)}\\(import\\.meta\\.url\\)`
  );
  const varMatch = source.match(varRe);
  if (!varMatch) return null;

  return varMatch[1];
}

/**
 * Identify the lazy init helper by scanning for the characteristic
 * `(A && (q = A(A = 0)), q)` shape in a variable declaration body.
 *
 * This helper is used by Bun for lazy module initialization and should
 * be excluded from module detection (it's runtime, not a module).
 */
export function identifyBunLazyInit(source: string): string | null {
  // The lazy init pattern: variable whose body contains the shape
  // `IDENT && (IDENT = IDENT(IDENT = 0))`
  const lazyRe = /(\w+)\s*&&\s*\(\w+\s*=\s*\1\(\1\s*=\s*0\)\)/;
  const match = source.match(lazyRe);
  if (!match) return null;

  // Walk backwards to find the binding name (may be comma-separated)
  const before = source.slice(0, match.index);
  const bindingMatch = before.match(
    /(?:(?:var|let|const)\s+|,)(\w+)\s*=\s*[^;]*$/
  );
  if (bindingMatch) return bindingMatch[1];

  return null;
}

function escapeRegExp(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}
