/**
 * Heuristic to detect whether a JavaScript identifier name looks minified.
 *
 * Catches patterns produced by various bundlers:
 * - Terser/uglify: single chars (a, b, c)
 * - Webpack: 2-char names (Ab, xY)
 * - Bun: 3-char mixed names (rlA, oGD, T5D, $aT)
 * - Rollup: short names with digits (q5aT, xRTd)
 */

/**
 * Common 2-character names that are NOT minified.
 */
const COMMON_2CHAR = new Set([
  "id", "fn", "cb", "el", "db", "io", "fs", "os", "vm", "ip",
  "ok", "on", "is", "no", "do", "to", "up", "of", "or", "if",
  "in", "at", "by", "go", "so", "op", "ul", "li", "td", "th",
  "tr", "br", "hr", "px",
]);

/**
 * Common 3-character names that are NOT minified.
 * Covers standard JS APIs, common abbreviations, DOM, Node.js, etc.
 */
const COMMON_3CHAR = new Set([
  // JavaScript built-ins and common patterns
  "get", "set", "map", "run", "key", "val", "ref", "err", "msg",
  "req", "res", "src", "buf", "len", "idx", "url", "api", "dom",
  "app", "env", "log", "str", "num", "obj", "arr", "col", "row",
  "min", "max", "sum", "avg", "abs", "sin", "cos", "tan", "pow",
  "hex", "raw", "tag", "top", "end", "add", "del", "put", "pop",
  "has", "use", "try", "new", "old", "out", "mid", "sub", "div",
  "mod", "bit", "cwd", "pid", "uid", "gid", "tty", "ssh", "ssl",
  "tcp", "udp", "dns", "ftp", "rpc", "sql", "csv", "xml", "css",
  "svg", "png", "jpg", "gif", "bmp", "pdf", "doc", "txt", "tsx",
  "jsx", "vue", "elm", "asm", "pkg", "lib", "bin", "cmd", "cli",
  "dir", "tmp", "opt", "arg", "cfg", "ini", "ast", "ttl", "pad",
  "tab", "gap", "box", "pos", "dim", "rgb", "hsl", "red", "cmp",
  "not", "and", "xor", "nil", "nan", "def", "var", "let", "eof",
  "nop", "ack", "syn", "fin", "seq", "dup", "cnt", "iff",
]);

/**
 * Returns true if the name looks like a minified/obfuscated identifier.
 *
 * Rules:
 * - 1 char: always minified (unless `_` or `$`)
 * - 2 chars: minified unless in allowlist
 * - 3 chars: check allowlist, then check for minified patterns
 *   (digits, unusual mixed-case like nGD, $ or _ prefix)
 * - 4 chars: minified only if has digits or unusual casing
 * - 5+ chars: not minified
 */
export function looksMinified(name: string): boolean {
  const len = name.length;

  if (len === 0) return false;

  if (len === 1) {
    // Single underscore or dollar sign are common conventions, not minified
    if (name === "_" || name === "$") return false;
    return true;
  }

  if (len === 2) {
    return !COMMON_2CHAR.has(name);
  }

  if (len === 3) {
    if (COMMON_3CHAR.has(name)) return false;
    return has3CharMinifiedPattern(name);
  }

  if (len === 4) {
    return has4CharMinifiedPattern(name);
  }

  // 5+ chars: not minified
  return false;
}

/**
 * Checks if a 3-char name (not in allowlist) has patterns typical of minified code.
 * More aggressive than 4-char since most real 3-char names are in the allowlist.
 */
function has3CharMinifiedPattern(name: string): boolean {
  // Contains digits → minified (T5D, a2b)
  if (/\d/.test(name)) return true;

  // Starts with $ or _ followed by uppercase → minified ($aT, _Gx)
  if (/^[$_][A-Z]/.test(name)) return true;

  // Multiple consecutive uppercase after lowercase (oGD, xRT)
  if (/[a-z][A-Z]{2,}/.test(name)) return true;

  // Ends with uppercase after lowercase (rlA, HaT)
  // For 3-char names not in the allowlist, this is very suspicious
  if (/[a-z][A-Z]$/.test(name)) return true;

  return false;
}

/**
 * Checks if a 4-char name has patterns typical of minified code.
 * More conservative than 3-char — many real 4-char names exist.
 */
function has4CharMinifiedPattern(name: string): boolean {
  // Contains digits → minified (q5aT, a2b3)
  if (/\d/.test(name)) return true;

  // Starts with $ or _ followed by uppercase → minified ($aBc, _Gxy)
  if (/^[$_][A-Z]/.test(name)) return true;

  // Multiple consecutive uppercase after lowercase (xRTd, nGDe)
  // This catches minified patterns but not normal camelCase (getX, setY)
  if (/[a-z][A-Z]{2,}/.test(name)) return true;

  return false;
}
