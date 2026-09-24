// Copy (READ ONLY on the source) the standing-cache entries the four oracle
// pairs' cache-keys.jsonl name into a scratch cache dir.
import fs from "node:fs";
import path from "node:path";
const src = "/work/neutrality-cache";
const dst = "/tmp/wp41/cache";
const pairs = ["2.1.85-2.1.86", "2.1.118-2.1.119", "2.1.197-2.1.198", "2.1.215-2.1.216"];
let copied = 0;
let absent = 0;
const seen = new Set();
for (const pair of pairs) {
  const rows = fs
    .readFileSync(`/work/oracle/oracle-b53b3a8/dumps/${pair}/cache-keys.jsonl`, "utf8")
    .split("\n")
    .filter(Boolean);
  for (const line of rows) {
    const key = JSON.parse(line).cacheKey;
    if (seen.has(key)) continue;
    seen.add(key);
    const rel = path.join(key.slice(0, 2), `${key.slice(2)}.json`);
    const from = path.join(src, rel);
    if (!fs.existsSync(from)) {
      absent++;
      continue;
    }
    fs.mkdirSync(path.join(dst, key.slice(0, 2)), { recursive: true });
    fs.copyFileSync(from, path.join(dst, rel));
    copied++;
  }
}
console.log(`unique keys=${seen.size} copied=${copied} absent=${absent}`);
