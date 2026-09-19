// WP1.2's Babel-side counts: bindings/scopes per the babel scope crawl over
// the same TS-beautified texts the Rust ingest counted — the comparison
// table's left column.
import { readFileSync, writeFileSync } from "node:fs";
import * as babel from "@babel/core";

const pairs = process.argv[2] ? [process.argv[2]] : ["2.1.85-2.1.86", "2.1.118-2.1.119", "2.1.197-2.1.198", "2.1.215-2.1.216"];
const rows = [];
for (const pair of pairs) {
  const oracle = process.env.ORACLE_ROOT || "/work/oracle/oracle-0294b28";
  const path = `${oracle}/dumps/${pair}/text/fresh.js`;
  let code;
  try {
    code = readFileSync(path, "utf8");
  } catch {
    console.log(`${pair}: fresh.js not ready`);
    continue;
  }
  const ast = babel.parseSync(code, {
    sourceType: "unambiguous",
    configFile: false,
    babelrc: false
  });
  let symbols = 0;
  let scopes = 0;
  let references = 0;
  babel.traverse(ast, {
    Scope(path) {
      scopes++;
      const bs = path.scope.bindings;
      symbols += Object.keys(bs).length;
      for (const [name, b] of Object.entries(bs)) {
        references += b.referencePaths.length + b.constantViolations.length;
      }
    }
  });
  rows.push({ pair, symbols, scopes, references });
  console.log(`${pair}: babel symbols=${symbols} scopes=${scopes} references=${references}`);
}
writeFileSync("/work/oracle/babel-counts.json", JSON.stringify(rows, null, 2));
