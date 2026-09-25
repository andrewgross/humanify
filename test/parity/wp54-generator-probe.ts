// WP5.4 probe: the TS's `using`-desugar printing, file by file, for the
// Rust port's generator gate (`humanify retain-lines`).
//
//   npx tsx test/parity/wp54-generator-probe.ts <tree> <out> [--desugar]
//
// For every .js/.cjs under <tree> (the desugar's own file walk: no
// node_modules, no .humanify):
//   default     — Babel's transformSync with NO plugins and the desugar's
//                 generator options (retainLines, compact false): the
//                 printer alone, on every file, a corpus far wider than
//                 the files that declare `using`;
//   --desugar   — `desugarUsing(code)` itself (files it leaves alone are
//                 not written).
// A file with comments is listed in <out>/.skipped (the Rust refuses
// comments: Babel's attachment is not ported), as is a Babel throw.
import fs from "node:fs";
import path from "node:path";
import { parseSync, transformSync } from "@babel/core";
import { jsFilesUnder } from "../../src/split/runnable-scaffold.js";
import { desugarUsing } from "../../src/split/using-desugar.js";

async function main(): Promise<void> {
  const [tree, out, flag] = process.argv.slice(2);
  if (!tree || !out) throw new Error("usage: see header");
  const desugar = flag === "--desugar";
  const skipped: string[] = [];
  let written = 0;
  for (const abs of (await jsFilesUnder(tree)).sort()) {
    const rel = path.relative(tree, abs);
    const code = fs.readFileSync(abs, "utf-8");
    let result: string | null;
    try {
      const ast = parseSync(code, {
        sourceType: "unambiguous",
        configFile: false,
        babelrc: false
      });
      const hasComments = Boolean(ast?.comments?.length);
      // --desugar: a file the desugar leaves alone is never printed, so its
      // comments do not matter — only a commented file it REWRITES is one
      // the Rust refuses.
      if (hasComments && (!desugar || desugarUsing(code) !== null)) {
        skipped.push(`${rel}\tcomments`);
        continue;
      }
      result = desugar
        ? desugarUsing(code)
        : (transformSync(code, {
            plugins: [],
            configFile: false,
            babelrc: false,
            sourceType: "unambiguous",
            retainLines: true,
            compact: false
          })?.code ?? null);
    } catch (err) {
      skipped.push(`${rel}\tthrow ${(err as Error).message.split("\n")[0]}`);
      continue;
    }
    if (result === null) continue;
    const dest = path.join(out, rel);
    fs.mkdirSync(path.dirname(dest), { recursive: true });
    fs.writeFileSync(dest, result);
    written++;
  }
  fs.mkdirSync(out, { recursive: true });
  fs.writeFileSync(path.join(out, ".skipped"), skipped.join("\n"));
  console.log(`written ${written}, skipped ${skipped.length}`);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
