/**
 * exp164: per-BINDING census of the hop's output trees — every binding
 * whose name is in the diag's mintedCensus.names, DECLARED in each file,
 * bucketed by vendor package / src / vendor-root. argv: <label> [treeName]
 */
import * as fs from "node:fs";
import * as path from "node:path";
import * as parser from "@babel/parser";
import traverseMod from "@babel/traverse";

const LABEL = process.argv[2] ?? "exp161-names-hop";
const TREE = process.argv[3] ?? "2.1.216";
const ROOT = `/tmp/eval-work/${LABEL}/${TREE}`;
const diag = JSON.parse(
  fs.readFileSync(`/tmp/eval-work/${LABEL}/2.1.216.diag.json`, "utf8"),
);
const minted = new Set(diag.coverage.mintedCensus.names);
console.log(`${LABEL}/${TREE}: ${minted.size} minted names`);

function walk(dir: string, out: string[]): string[] {
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, e.name);
    if (e.isDirectory()) {
      if (e.name === ".humanify" || e.name === "node_modules") continue;
      walk(p, out);
    } else if (/\.(js|cjs)$/.test(e.name)) out.push(p);
  }
  return out;
}
const files = walk(ROOT, []);

function pkgOf(rel: string): string {
  const parts = rel.split("/");
  if (parts[0] === "vendor") {
    if (parts.length === 1) return "vendor-root";
    return parts[1].startsWith("@") ? `${parts[1]}/${parts[2]}` : parts[1];
  }
  return parts[0];
}

const rows: { name: string; rel: string; pkg: string }[] = [];
let parseFails = 0;
for (const f of files) {
  const rel = path.relative(ROOT, f);
  const text = fs.readFileSync(f, "utf8");
  let ast: parser.ParseResult<import("@babel/types").File>;
  try {
    ast = parser.parse(text, { sourceType: "unambiguous", errorRecovery: true });
  } catch (e) {
    parseFails += 1;
    console.error("PARSE FAIL", rel, String(e).slice(0, 120));
    continue;
  }
  const declared = new Set<string>();
  traverseMod.default(ast, {
    Scopable: (p: { scope: { bindings: Record<string, unknown> } }) => {
      for (const name of Object.keys(p.scope.bindings)) declared.add(name);
    },
  });
  for (const name of declared) {
    if (minted.has(name)) rows.push({ name, rel, pkg: pkgOf(rel) });
  }
}
const byPkg = new Map<string, number>();
for (const r of rows) byPkg.set(r.pkg, (byPkg.get(r.pkg) ?? 0) + 1);
const sorted = [...byPkg.entries()].sort((a, b) => b[1] - a[1]);
console.log("--- junk binding declarations by bucket ---");
for (const [p, c] of sorted) console.log(`${c}\t${p}`);
console.log("parse fails:", parseFails, "rows:", rows.length);
fs.writeFileSync(
  `/work/exp164-mint-census/decls-${LABEL}-${TREE}.json`,
  JSON.stringify(rows),
);
