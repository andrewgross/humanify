import * as fs from "node:fs";
const d = JSON.parse(fs.readFileSync("/work/exp164-mint-census/dump-run/exp164.diag.json", "utf8"));
console.log("top keys:", Object.keys(d));
for (const k of Object.keys(d)) {
  const v = (d as any)[k];
  if (Array.isArray(v)) console.log(k, `array(${v.length})`, JSON.stringify(v[0]).slice(0, 200));
  else if (v && typeof v === "object") console.log(k, "obj:", Object.keys(v).slice(0, 25).join(","));
  else console.log(k, JSON.stringify(v).slice(0, 120));
}
