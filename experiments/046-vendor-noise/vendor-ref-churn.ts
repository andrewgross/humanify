/**
 * How much of the APP-CODE diff is caused purely by the names that reach into
 * vendored libraries?
 *
 * Not the noise inside vendor/ — that is out of scope. This asks: of the lines
 * that changed in src/, how many are byte-identical once you mask the
 * identifiers that name a vendored module?
 *
 * Two families reach into vendor/ from app code:
 *   1. EMITTER aliases — `const lib_eb5345cb_2 = require(".../vendor/noop/lib_eb5345cb.js")`.
 *      Minted by cjs-emit's buildNsVars from the file path, never seen by the
 *      renamer's stability machinery.
 *   2. RENAMER handles — `React93 = importDefault(lib_eb5345cb_2.f(), 1)`, a
 *      module-level binding in an app file holding the imported namespace. Named
 *      by the rename pipeline, which appends a slot ordinal because ~120 of them
 *      collide on the stem `React`.
 *
 * PREDICATE, stated plainly so it can be checked against the claim: a changed
 * line PAIR counts only when the two sides become IDENTICAL after every
 * vendor-family identifier on both sides is replaced with the same placeholder.
 * A line that also changed for any other reason does NOT count. So this is a
 * FLOOR on vendor-reference churn, not an estimate of it.
 *
 * MEASURED (exp043-nearident trees): 660 / 22 / 544 / 62 git lines on the four
 * gate hops -- 1,288 total, ~8% of measured src noise. It is a SLICE of the
 * `naming` bucket, not additive with it.
 *
 * This first reported 826 lines because it paired diff lines POSITIONALLY within
 * each -U0 hunk and skipped any hunk whose - and + counts differed, discarding
 * 79% of the diff. Matching by masked content across the whole FILE fixed it.
 * See "How this brief was measured" in README.md.
 *
 * Usage: npx tsx vendor-ref-churn.ts <priorSrcDir> <freshSrcDir> <label>
 */
import { execFileSync } from "node:child_process";
import * as fs from "node:fs";
import * as path from "node:path";

function walk(dir: string, base = dir, out: string[] = []): string[] {
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, e.name);
    if (e.isDirectory()) walk(p, base, out);
    else if (e.name.endsWith(".js")) out.push(path.relative(base, p));
  }
  return out;
}

/** Names bound to a vendored module, both families, across a whole tree. */
function vendorNames(srcDir: string): Set<string> {
  const aliases = new Set<string>();
  const derived = new Set<string>();
  const files = walk(srcDir);
  // Pass 1: direct requires of a vendor path.
  const perFile = new Map<string, string>();
  for (const rel of files) {
    const code = fs.readFileSync(path.join(srcDir, rel), "utf8");
    perFile.set(rel, code);
    for (const m of code.matchAll(
      /(?:const|let|var)\s+([A-Za-z0-9_$]+)\s*=\s*require\("([^"]*vendor\/[^"]*)"\)/g
    )) {
      aliases.add(m[1]);
    }
  }
  // Pass 2: handles initialised FROM one of those aliases, e.g.
  //   React93 = (0, esmoduleConverter.importDefault)(lib_eb5345cb_2.f(), 1);
  for (const code of perFile.values()) {
    for (const m of code.matchAll(
      /^\s*([A-Za-z0-9_$]+)\s*=\s*([^;\n]+);?$/gm
    )) {
      const [, name, rhs] = m;
      if (aliases.has(name)) continue;
      for (const a of rhs.matchAll(/\b([A-Za-z0-9_$]+)\b/g)) {
        if (aliases.has(a[1])) {
          derived.add(name);
          break;
        }
      }
    }
  }
  return new Set([...aliases, ...derived]);
}

function mask(line: string, names: Set<string>): string {
  return line.replace(/\b[A-Za-z0-9_$]+\b/g, (w) => (names.has(w) ? "«V»" : w));
}

function main(): void {
  const [priorSrc, freshSrc, label] = process.argv.slice(2);
  const names = new Set([...vendorNames(priorSrc), ...vendorNames(freshSrc)]);

  let raw = "";
  try {
    raw = execFileSync(
      "git",
      ["diff", "--no-index", "-U0", "--no-color", priorSrc, freshSrc],
      { encoding: "utf8", maxBuffer: 1 << 30 }
    );
  } catch (e) {
    raw = (e as { stdout?: string }).stdout ?? "";
  }

  let changed = 0;
  let vendorOnly = 0;
  let hunks = 0;
  let pairs = 0;
  let minus: string[] = [];
  let plus: string[] = [];

  const flush = () => {
    changed += minus.length + plus.length;
    if (minus.length + plus.length > 0) hunks++; // files, now
    // Match by MASKED CONTENT across the whole FILE, not per hunk. With -U0 a
    // renamed identifier produces one tiny hunk per use site, so a line's true
    // partner is almost never in the same hunk; positional pairing discarded
    // 79% of changed lines and per-hunk matching saw only 316 of 44,824.
    const pool = new Map<string, string[]>();
    for (const p of plus) {
      const k = mask(p, names);
      (pool.get(k) ?? pool.set(k, []).get(k)!).push(p);
    }
    for (const m of minus) {
      const k = mask(m, names);
      const bucket = pool.get(k);
      if (!bucket || bucket.length === 0) continue;
      const partner = bucket.pop() as string;
      pairs++;
      if (partner !== m) vendorOnly += 2;
    }
    minus = [];
    plus = [];
  };

  for (const line of raw.split("\n")) {
    if (line.startsWith("--- ") || line.startsWith("+++ ")) continue;
    if (line.startsWith("diff --git")) {
      flush();
      continue;
    }
    if (line.startsWith("@@")) continue;
    if (line.startsWith("-")) minus.push(line.slice(1));
    else if (line.startsWith("+")) plus.push(line.slice(1));
  }
  flush();

  const pct = changed ? (100 * vendorOnly) / changed : 0;
  console.log(`=== VENDOR-REFERENCE CHURN IN APP CODE — ${label ?? ""} ===`);
  console.log(`  vendor-bound identifiers in scope: ${names.size}`);
  console.log(`  src changed lines: ${changed}`);
  console.log(
    `  changed ONLY by a vendor-reference name: ${vendorOnly} (${pct.toFixed(1)}%)`
  );
  console.log(`  files with churn ${hunks}, masked-content matches ${pairs}`);
  console.log(`ROW|${label ?? ""}|${changed}|${vendorOnly}`);
}

main();
