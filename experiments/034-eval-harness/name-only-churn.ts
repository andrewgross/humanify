/**
 * Name-only churn: diff lines whose code is identical once local identifiers
 * are masked. Same code, different name.
 *
 * ## Why this exists next to `layout.naming` rather than replacing it
 *
 * `composeDiff` charges `naming` only when a statement's HASH is unchanged and
 * its text differs. A statement that carries BOTH a real edit and a rename has
 * a flipped hash, so it falls to the edited-pair path and is charged ENTIRELY
 * to `real` — rename included. That is deliberate and documented there; it is
 * also why `naming` reads 962 on 2.1.215->216 while the line-level truth is
 * ~6,000. exp055 found the same shape and estimated 3,448.
 *
 * Changing `naming`'s definition would invalidate every committed reference, so
 * this is a second, independent, LINE-level view reported alongside.
 *
 * ## What is masked, and what deliberately is not
 *
 * Only identifiers that are neither preceded by `.` nor followed by `:` — a
 * property access or an object key is SEMANTIC. Masking those made
 * `userContentItem.tool_use_id` -> `contentItem.is_error` look like a rename
 * when the property genuinely changed, and put a first estimate at 41.5%
 * against a true 21.9%.
 *
 * Lines whose text is unchanged are removed first: they are not edits at all,
 * and counting them inflated the same estimate.
 */
import * as fs from "node:fs";
import * as path from "node:path";

/** JS keywords, never masked — masking them would equate different control flow. */
const KEYWORDS = new Set(
  `var let const function return if else for while do switch case break continue
   new typeof instanceof in of delete void null true false this async await yield
   throw try catch finally class extends super import export from default require
   module exports static get set`.split(/\s+/)
);

/**
 * `(\.\s*)?ident(\s*:)?` — the optional groups are what protect property names
 * and object keys from masking.
 */
const TOKEN = /(\.\s*)?([A-Za-z_$][A-Za-z0-9_$]*)(\s*:)?/g;

function maskIdentifiers(line: string): string {
  return line
    .replace(TOKEN, (all, dot, name, colon) =>
      dot || colon || KEYWORDS.has(name) ? all : `${dot ?? ""}X${colon ?? ""}`
    )
    .trim();
}

export interface NameOnlyChurn {
  /** git lines (deletions + insertions) whose code differs only in names. */
  lines: number;
  /** Files holding at least one. */
  files: number;
  /** Changed lines considered, for a share. */
  changedLines: number;
}

function walkFiles(dir: string, base = dir, out: string[] = []): string[] {
  if (!fs.existsSync(dir)) return out;
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, e.name);
    if (e.isDirectory()) walkFiles(p, base, out);
    else if (e.name.endsWith(".js")) out.push(path.relative(base, p));
  }
  return out;
}

/** Multiset difference: entries of `a` not covered by `b`. */
function surplus(a: string[], b: string[]): string[] {
  const counts = new Map<string, number>();
  for (const l of b) counts.set(l, (counts.get(l) ?? 0) + 1);
  const out: string[] = [];
  for (const l of a) {
    const n = counts.get(l) ?? 0;
    if (n > 0) counts.set(l, n - 1);
    else out.push(l);
  }
  return out;
}

/**
 * Count name-only churn between two emitted trees.
 *
 * Multiset difference rather than a positional diff, so the answer does not
 * depend on how a diff algorithm happens to align hunks — the same reasoning as
 * `buildConstantChurn`, and it matters for a number meant to be compared across
 * runs.
 */
export function nameOnlyChurn(
  priorDir: string,
  freshDir: string
): NameOnlyChurn {
  const files = new Set([...walkFiles(priorDir), ...walkFiles(freshDir)]);
  let lines = 0;
  let changedLines = 0;
  let touched = 0;

  for (const rel of files) {
    const pPath = path.join(priorDir, rel);
    const fPath = path.join(freshDir, rel);
    if (!fs.existsSync(pPath) || !fs.existsSync(fPath)) continue;
    const prior = fs.readFileSync(pPath, "utf8").split("\n");
    const fresh = fs.readFileSync(fPath, "utf8").split("\n");

    // Lines identical on both sides are not edits, whatever the hunks say.
    const gone = surplus(prior, fresh);
    const came = surplus(fresh, prior);
    if (gone.length === 0 || came.length === 0) {
      changedLines += gone.length + came.length;
      continue;
    }
    changedLines += gone.length + came.length;

    const masked = new Map<string, number>();
    for (const l of gone) {
      const k = maskIdentifiers(l);
      if (k.length === 0) continue;
      masked.set(k, (masked.get(k) ?? 0) + 1);
    }
    let fileLines = 0;
    for (const l of came) {
      const k = maskIdentifiers(l);
      const n = masked.get(k) ?? 0;
      if (n === 0) continue;
      masked.set(k, n - 1);
      fileLines += 2; // the deletion and the insertion
    }
    if (fileLines > 0) {
      lines += fileLines;
      touched++;
    }
  }

  return { lines, files: touched, changedLines };
}
