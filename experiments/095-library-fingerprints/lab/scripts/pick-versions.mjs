// usage: node pick-versions.mjs  -> writes ../versions.json
// For each catalog package pick up to 4 published versions:
//   latest L; "patch" = highest earlier patch of L's minor; "minor" = highest of the
//   previous minor in L's major; "major" = highest version of the previous major.
import fs from "node:fs";
import { execFileSync } from "node:child_process";
import path from "node:path";
const here = path.dirname(new URL(import.meta.url).pathname);
const cat = JSON.parse(fs.readFileSync(path.join(here, "catalog.json"), "utf8"));
const parse = (v) => v.split(".").map(Number);
const cmp = (a, b) => { const x = parse(a), y = parse(b); for (let i = 0; i < 3; i++) if (x[i] !== y[i]) return x[i] - y[i]; return 0; };
const out = [];
for (const p of cat.packages) {
  const info = JSON.parse(execFileSync("npm", ["view", p.name, "versions", "dist-tags", "--json"], { encoding: "utf8" }));
  const latest = info["dist-tags"].latest;
  const vs = info.versions.filter((v) => /^\d+\.\d+\.\d+$/.test(v) && cmp(v, latest) <= 0).sort(cmp);
  const [M, m, pt] = parse(latest);
  const pick = { latest };
  const patch = vs.filter((v) => { const [a, b, c] = parse(v); return a === M && b === m && c < pt; }).pop();
  if (patch) pick.patch = patch;
  const minor = vs.filter((v) => { const [a, b] = parse(v); return a === M && b < m; }).pop();
  if (minor) pick.minor = minor;
  const major = vs.filter((v) => parse(v)[0] < M).pop();
  if (major) pick.major = major;
  out.push({ name: p.name, versions: pick });
  console.log(p.name, JSON.stringify(pick));
}
fs.writeFileSync(path.join(here, "..", "versions.json"), JSON.stringify(out, null, 1));
