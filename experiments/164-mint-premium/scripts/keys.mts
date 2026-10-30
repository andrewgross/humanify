import * as fs from "node:fs";
const d = JSON.parse(fs.readFileSync("/work/exp164-mint-census/dump-run/exp164.diag.json", "utf8"));
const ld = d.libraryDiscovery;
console.log("libraryDiscovery keys:", Object.keys(ld));
for (const k of Object.keys(ld)) {
  const v = ld[k];
  console.log(k, Array.isArray(v) ? `array(${v.length})` : typeof v, Array.isArray(v) && v.length ? JSON.stringify(v[0]).slice(0, 300) : "");
}
