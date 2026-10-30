import * as fs from "node:fs";
const dump = JSON.parse(fs.readFileSync("/work/exp164-mint-census/dump-run/exp164.diag.json", "utf8"));
const er = dump.eraRestoration;
const led = JSON.parse(
  fs.readFileSync("/work/exp164-mint-census/dump-run/tree/.humanify/split-ledger.json", "utf8"),
);
const junk: { name: string; stmt: number; file: string }[] = JSON.parse(
  fs.readFileSync("/work/exp164-mint-census/shipped-located-exp161-names-hop.json", "utf8"),
);
console.log(`ledger order ${led.order.length}, bodyLen ${er.bodyLen}, emitNames ${led.emitNames.length}`);

// vendor ordinals
const vendorStmts = new Map<number, string>();
for (let i = 0; i < led.order.length; i++)
  if (led.order[i].startsWith("vendor/")) vendorStmts.set(i, led.order[i]);
const vendorFiles = new Set(vendorStmts.values());
console.log(`vendor statements ${vendorStmts.size} in ${vendorFiles.size} files`);

const frozenByStmt = new Map<number, Set<string>>();
for (const [stmt, , name] of er.frozen) {
  const s = frozenByStmt.get(stmt) ?? new Set();
  s.add(name);
  frozenByStmt.set(stmt, s);
}
console.log(`frozen rows ${er.frozen.length} over ${frozenByStmt.size} statements; frozen stmts in vendor: ${[...frozenByStmt.keys()].filter(s => vendorStmts.has(s)).length}`);

// junk bindings per vendor statement
const junkByStmt = new Map<number, string[]>();
for (const r of junk) {
  if (!r.file.startsWith("vendor/")) continue;
  const a = junkByStmt.get(r.stmt) ?? [];
  a.push(r.name);
  junkByStmt.set(r.stmt, a);
}
let both = 0, onlyJunk = 0, frozenNamesInJunkStmt = 0;
const unfrozen: string[] = [];
for (const [stmt, names] of [...junkByStmt.entries()].sort((a, b) => a[0] - b[0])) {
  const fr = frozenByStmt.get(stmt);
  const inF = names.filter(n => fr?.has(n));
  both += inF.length;
  onlyJunk += names.length - inF.length;
  if (!fr || fr.size === 0) {
    unfrozen.push(
      `stmt ${stmt} ${vendorStmts.get(stmt)} junk [${names.join(",")}] emitNames [${led.emitNames[stmt]}] frozen NONE`,
    );
  } else {
    frozenNamesInJunkStmt += fr.size;
  }
}
console.log(`vendor junk bindings ${[...junkByStmt.values()].reduce((a, b) => a + b.length, 0)} over ${junkByStmt.size} statements`);
console.log(`junk names also frozen: ${both}, junk names NOT frozen: ${onlyJunk}, frozen rows in junk stmts: ${frozenNamesInJunkStmt}`);
// frozen rows in vendor statements with NO junk
const frozenNoJunk = [...frozenByStmt.keys()].filter(s => vendorStmts.has(s) && !junkByStmt.has(s));
console.log(`frozen vendor stmts with no junk: ${frozenNoJunk.length}`);
console.log(`\nvendor statements with junk but NO frozen rows (${unfrozen.length}):`);
for (const l of unfrozen.slice(0, 25)) console.log("  " + l);
// also: emitNames vs junk for a frozen stmt example
const fEx = [...frozenByStmt.keys()].filter(s => vendorStmts.has(s) && junkByStmt.has(s)).slice(0, 5);
for (const s of fEx) {
  console.log(`\nfrozen+juke stmt ${s} ${vendorStmts.get(s)}: junk [${junkByStmt.get(s)}] frozen [${[...frozenByStmt.get(s)!]}] emitNames [${led.emitNames[s]}]`);
}
