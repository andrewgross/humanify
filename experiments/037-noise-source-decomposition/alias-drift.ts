/**
 * Require-alias (namespace-variable) drift — exp037 Finding 4.
 *
 * Each emitted file imports its cross-file dependencies as
 * `const <alias> = require("<relative path>");`. The alias is chosen by
 * `nsCandidates` in cjs-emit.ts: the bare basename first (`fileModTime`), then
 * widening up the path (`lspSearchFileModTime`), taking the FIRST candidate that
 * is "free". Freeness is checked against the CURRENT bundle — `inSource` rules
 * out any identifier appearing ANYWHERE in the bundle, including a nested local
 * in an unrelated file. So one LLM local-variable naming draw can flip a bare
 * alias to a widened one, rewriting EVERY reference to that module in EVERY
 * importing file. The choice is not prior-aware.
 *
 * This measures the cost: for each file, which imports kept their PATH but
 * changed their ALIAS, and how many lines that drags.
 *
 * Usage: npx tsx alias-drift.ts <priorSrcDir> <freshSrcDir>
 */
import * as fs from "node:fs";
import * as path from "node:path";

const REQUIRE_LINE = /^const ([A-Za-z_$][\w$]*) = require\("([^"]+)"\);$/;

function walk(dir: string, base = dir, out: string[] = []): string[] {
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, e.name);
    if (e.isDirectory()) walk(p, base, out);
    else if (e.name.endsWith(".js")) out.push(path.relative(base, p));
  }
  return out;
}

/** path -> alias for one emitted file's require header. */
function aliasesOf(code: string): Map<string, string> {
  const map = new Map<string, string>();
  for (const line of code.split("\n")) {
    const m = REQUIRE_LINE.exec(line);
    if (m) map.set(m[2], m[1]);
    // header ends at the first non-header line; keep scanning cheaply anyway
  }
  return map;
}

function countRefLines(code: string, alias: string): number {
  const re = new RegExp(`\\b${alias.replace(/\$/g, "\\$")}\\b`);
  let n = 0;
  for (const line of code.split("\n")) if (re.test(line)) n++;
  return n;
}

function main() {
  const [priorDir, freshDir] = process.argv.slice(2);
  const priorFiles = new Set(walk(priorDir));
  const freshFiles = new Set(walk(freshDir));
  const common = [...freshFiles].filter((f) => priorFiles.has(f));

  let filesAffected = 0;
  let renameCount = 0;
  let draggedLines = 0;
  const byPair = new Map<string, { files: number; lines: number }>();

  for (const f of common) {
    const priorCode = fs.readFileSync(path.join(priorDir, f), "utf8");
    const freshCode = fs.readFileSync(path.join(freshDir, f), "utf8");
    const pa = aliasesOf(priorCode);
    const fa = aliasesOf(freshCode);
    let touched = false;
    for (const [imp, freshAlias] of fa) {
      const priorAlias = pa.get(imp);
      if (priorAlias === undefined || priorAlias === freshAlias) continue;
      // Same imported PATH, different alias -> pure alias drift.
      renameCount++;
      touched = true;
      const lines = countRefLines(freshCode, freshAlias);
      draggedLines += lines;
      const key = `${priorAlias} -> ${freshAlias}`;
      const e = byPair.get(key) ?? { files: 0, lines: 0 };
      e.files++;
      e.lines += lines;
      byPair.set(key, e);
    }
    if (touched) filesAffected++;
  }

  console.log(
    "=== REQUIRE-ALIAS DRIFT (same import path, different alias) ==="
  );
  console.log(`  files affected:        ${filesAffected} / ${common.length}`);
  console.log(`  alias renames:         ${renameCount}`);
  console.log(`  reference lines dragged: ${draggedLines}`);
  console.log("\n=== top alias renames by lines dragged ===");
  const sorted = [...byPair.entries()].sort((a, b) => b[1].lines - a[1].lines);
  for (const [k, e] of sorted.slice(0, 25)) {
    console.log(
      `  ${String(e.lines).padStart(5)} ln  in ${String(e.files).padStart(3)} files   ${k}`
    );
  }
  console.log(`\n  distinct alias-rename pairs: ${byPair.size}`);
}

main();
