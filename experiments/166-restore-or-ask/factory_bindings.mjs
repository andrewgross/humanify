// exp166 walkthrough, part 2: every binding declared in the VENDORED FACTORY
// files of a tree (the unpack's bodies, which ship unformatted and unnamed
// except where the index restored them), with the same file's bindings in
// the prior tree by declaration order.
//
// usage: node factory_bindings.mjs <tree> <prior-tree> <files.json> > out.jsonl
//   files.json: ["vendor/…/x.js", …] — the factory files to read (the diag's
//   eraRestoration.factoryFiles paths).
import { createRequire } from "node:module";
import * as fs from "node:fs";
import * as path from "node:path";

const require = createRequire(new URL("../../package.json", import.meta.url));
const parser = require("@babel/parser");
const traverse = require("@babel/traverse").default;

function bindings(file) {
  let code;
  try {
    code = fs.readFileSync(file, "utf8");
  } catch {
    return null;
  }
  let ast;
  try {
    ast = parser.parse(code, { sourceType: "unambiguous", errorRecovery: true });
  } catch {
    return null;
  }
  const out = [];
  traverse(ast, {
    Scope(p) {
      for (const [name, b] of Object.entries(p.scope.bindings)) {
        if (b.__seen) continue;
        b.__seen = true;
        out.push({ name, at: b.identifier.start, kind: b.kind });
      }
    }
  });
  out.sort((a, b) => a.at - b.at);
  return out;
}

const [tree, prior, filesJson] = process.argv.slice(2);
const files = JSON.parse(fs.readFileSync(filesJson, "utf8"));
for (const rel of files) {
  const now = bindings(path.join(tree, rel));
  if (!now) continue;
  const before = bindings(path.join(prior, rel));
  now.forEach((b, i) => {
    const was = before && before.length === now.length ? before[i].name : null;
    process.stdout.write(
      `${JSON.stringify({ file: rel, name: b.name, kind: b.kind, priorSamePath: before !== null, priorName: was })}\n`
    );
  });
}
