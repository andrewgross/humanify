// usage: node scripts/pick.mjs [N=2000]
// Picks the index ROOTS: the npm top-download list (npm-high-impact, same list the exp095 fresh corpus used),
// minus (a) type-only / platform-binary packages, (b) the HELD-OUT roots of the three foreign corpora
// (subtree-hashes/verify/flab 83 foreign, immutable-literals/verify/neg 75, exact-module-hash/verify/fresh 400),
// so those corpora stay foreign. Then picks versions per root from the registry:
//   latest; the previous patch of latest (same major.minor); the latest of each of the 2 previous majors
//   (for 0.x, the minor plays the major's role). Prereleases never.
// Writes data/roots.json (rank order) and data/versions.jsonl ({rank,name,role,version}).
import fs from "node:fs";
import { npmTopDownloads } from "/work/exp095-library-fingerprints/exact-module-hash/npmtop/node_modules/npm-high-impact/index.js";
const S = "/work/exp095-library-fingerprints/scale";
const N = Number(process.argv[2] || 2000);
const X = "/work/exp095-library-fingerprints";
const held = new Set();
for (const p of JSON.parse(fs.readFileSync(X + "/subtree-hashes/verify/flab/versions.json", "utf8"))) if (p.set === "foreign") held.add(p.name);
for (const f of [X + "/immutable-literals/verify/neg/list.txt", X + "/exact-module-hash/verify/fresh/sample.txt"])
  for (const l of fs.readFileSync(f, "utf8").split("\n")) if (l.trim()) held.add(l.trim());
const skip = /^@types\/|^@esbuild\/|^@rollup\/rollup-|^@swc\/core-|^@next\/swc-|^@img\/sharp-|-(darwin|linux|win32|freebsd|android)(-|$)|^@parcel\/watcher-|^@oxc-|^@biomejs\/cli-|^lightningcss-|^@tailwindcss\/oxide-|^fsevents$|^type-fest$|^csstype$|^undici-types$|^devtools-protocol$/;
const roots = []; const skipped = []; const heldHits = [];
for (let r = 0; r < npmTopDownloads.length && roots.length < N; r++) {
  const n = npmTopDownloads[r];
  if (held.has(n)) { heldHits.push({ rank: r + 1, name: n }); continue; }
  if (skip.test(n)) { skipped.push({ rank: r + 1, name: n }); continue; }
  roots.push({ rank: r + 1, name: n, pos: roots.length + 1 });
}
fs.mkdirSync(S + "/data/meta", { recursive: true });
const semver = (v) => { const m = /^(\d+)\.(\d+)\.(\d+)$/.exec(v); return m ? [+m[1], +m[2], +m[3]] : null; };
const cmp = (a, b) => a[0] - b[0] || a[1] - b[1] || a[2] - b[2];
async function meta(name) {
  const f = `${S}/data/meta/${name.replace("/", "__")}.json`;
  if (fs.existsSync(f)) return JSON.parse(fs.readFileSync(f, "utf8"));
  for (let t = 0; t < 4; t++) {
    try {
      const r = await fetch("https://registry.npmjs.org/" + name.replace("/", "%2f"), { headers: { accept: "application/vnd.npm.install-v1+json" } });
      if (r.status === 404) return null;
      const j = await r.json();
      const slim = { name, latest: j["dist-tags"]?.latest, versions: Object.keys(j.versions || {}), deprecated: Object.fromEntries(Object.entries(j.versions || {}).filter(([, v]) => v.deprecated).map(([k]) => [k, 1])) };
      fs.writeFileSync(f, JSON.stringify(slim));
      return slim;
    } catch (e) { await new Promise((res) => setTimeout(res, 1000 * (t + 1))); }
  }
  return null;
}
function pickVersions(m) {
  const L = semver(m.latest || ""); if (!L) return m.latest ? [["latest", m.latest]] : [];
  const vs = m.versions.map((v) => [v, semver(v)]).filter(([, s]) => s && cmp(s, L) <= 0);
  const key = (s) => (s[0] === 0 ? `0.${s[1]}` : `${s[0]}`); // 0.x: minor is the "major"
  const out = [["latest", m.latest]];
  const prevPatch = vs.filter(([, s]) => s[0] === L[0] && s[1] === L[1] && s[2] < L[2]).sort((a, b) => cmp(b[1], a[1]))[0];
  if (prevPatch) out.push(["patch", prevPatch[0]]);
  const majors = [...new Set(vs.map(([, s]) => key(s)))].filter((k) => k !== key(L))
    .sort((a, b) => cmp(semver(a.includes(".") ? a + ".0" : a + ".0.0"), semver(b.includes(".") ? b + ".0" : b + ".0.0"))).reverse();
  for (const [i, k] of majors.slice(0, 2).entries()) {
    const best = vs.filter(([, s]) => key(s) === k).sort((a, b) => cmp(b[1], a[1]))[0];
    if (best) out.push([i === 0 ? "major-1" : "major-2", best[0]]);
  }
  return out;
}
let i = 0; const rows = [];
await Promise.all(Array.from({ length: 16 }, async () => {
  while (i < roots.length) { const r = roots[i++]; const m = await meta(r.name); r.meta = !!m; if (m) for (const [role, v] of pickVersions(m)) rows.push({ pos: r.pos, rank: r.rank, name: r.name, role, version: v }); }
}));
rows.sort((a, b) => a.pos - b.pos || a.role.localeCompare(b.role));
fs.writeFileSync(S + "/data/roots.json", JSON.stringify({ N, held: [...held].sort(), heldHits, skipped, roots: roots.map(({ meta: _m, ...r }) => r) }, null, 1));
fs.writeFileSync(S + "/data/versions.jsonl", rows.map((r) => JSON.stringify(r)).join("\n") + "\n");
console.log(`roots ${roots.length} (last rank ${roots.at(-1).rank}); held-out hits ${heldHits.length}; skipped ${skipped.length}; versions ${rows.length}; no-meta ${roots.filter((r) => !r.meta).length}`);
