// Unit-hash index of library code, and a voting classifier over it.
import { forEachLibUnit, isLib } from './load.mjs';

// opts: { mode, configs, minNodes, variant, exclude(row)->bool, modes }
export function buildIndex({ mode = 'loose', configs, minNodes = 20, variant = 'mask', exclude = () => false, modes = null, types = null }) {
  const H = new Map();      // hash -> { pkgs:Set, files:Set }
  const roots = new Map();  // whole-module hash -> Set(pkg/file)
  forEachLibUnit(mode, configs, (u, r) => {
    if (!isLib(r) || exclude(r)) return;
    if (u.type === 'root') { let s = roots.get(u[variant]); if (!s) roots.set(u[variant], s = new Set()); s.add(`${r.pkg}/${r.file}`); return; }
    if (u.nodes < minNodes) return;
    if (types && !types.includes(u.type)) return;
    let e = H.get(u[variant]); if (!e) H.set(u[variant], e = { pkgs: new Set(), files: new Set(), vers: new Set(), nodes: u.nodes });
    e.pkgs.add(r.pkg); e.files.add(`${r.pkg}/${r.file}`); e.vers.add(`${r.pkg}@${r.pkgVersion}`);
  }, { modes });
  let amb = 0; for (const e of H.values()) if (e.pkgs.size > 1) amb++;
  return { H, roots, minNodes, variant, stats: { hashes: H.size, ambiguous: amb, roots: roots.size } };
}

// Classify one query (a list of units). Returns { pkg, file, votes, second, matched, considered }
export function classify(idx, us, { k = 2, margin = 2, allowAmbiguous = false, big = Infinity } = {}) {
  const seen = new Set();
  const votes = new Map(), fileVotes = new Map(), maxNodes = new Map();
  let considered = 0, matched = 0;
  for (const u of us) {
    if (u.type === 'root' || u.nodes < idx.minNodes) continue;
    const h = u[idx.variant];
    if (seen.has(h)) continue; seen.add(h);
    considered++;
    const e = idx.H.get(h); if (!e) continue;
    if (e.pkgs.size > 1 && !allowAmbiguous) continue;
    matched++;
    for (const p of e.pkgs) { votes.set(p, (votes.get(p) || 0) + 1 / e.pkgs.size); maxNodes.set(p, Math.max(maxNodes.get(p) || 0, u.nodes)); }
    for (const f of e.files) fileVotes.set(f, (fileVotes.get(f) || 0) + 1 / e.files.size);
  }
  const sorted = [...votes].sort((a, b) => b[1] - a[1]);
  const [best, second] = [sorted[0] || [null, 0], sorted[1] || [null, 0]];
  const res = { votes: best[1], second: second[1], matched, considered, pkg: null, file: null, cand: best[0] };
  if ((best[1] >= k || (maxNodes.get(best[0]) || 0) >= big) && best[1] >= margin * second[1]) {
    res.pkg = best[0];
    const fs = [...fileVotes].filter(([f]) => f.startsWith(best[0] + '/')).sort((a, b) => b[1] - a[1]);
    res.file = fs.length ? fs[0][0] : null;
  }
  // exact whole-module hit
  const root = us.find(u => u.type === 'root');
  res.rootHit = root ? (idx.roots.get(root[idx.variant]) || null) : null;
  return res;
}

// union of several indexes (same minNodes / variant); a hash claimed by different packages becomes ambiguous
export function mergeIndex(...xs) {
  const H = new Map(), roots = new Map();
  for (const x of xs) {
    for (const [h, e] of x.H) {
      let m = H.get(h); if (!m) H.set(h, m = { pkgs: new Set(), files: new Set(), vers: new Set(), nodes: e.nodes });
      for (const p of e.pkgs) m.pkgs.add(p); for (const f of e.files) m.files.add(f); for (const v of e.vers) m.vers.add(v);
    }
    for (const [h, s] of x.roots) { let m = roots.get(h); if (!m) roots.set(h, m = new Set()); for (const f of s) m.add(f); }
  }
  let amb = 0; for (const e of H.values()) if (e.pkgs.size > 1) amb++;
  return { H, roots, minNodes: xs[0].minNodes, variant: xs[0].variant, stats: { hashes: H.size, ambiguous: amb, roots: roots.size } };
}
