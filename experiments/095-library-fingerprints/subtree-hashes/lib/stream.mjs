// Label a stream of module-level statements with library names, from unit-hash hits, then smooth.
// stmts: [{start,end,units:[...]}] in bundle order. Returns per-statement {pkg|null, hit:bool}
export function labelStream(idx, stmts, { gap = 3, minHits = 1 } = {}) {
  const lab = stmts.map(s => {
    const votes = new Map();
    for (const u of s.units) {
      if (u.type === 'root' || u.nodes < idx.minNodes) continue;
      const e = idx.H.get(u[idx.variant]); if (!e || e.pkgs.size > 1) continue;
      const p = [...e.pkgs][0]; votes.set(p, (votes.get(p) || 0) + 1);
    }
    if (!votes.size) return { pkg: null, hit: false };
    const sorted = [...votes].sort((a, b) => b[1] - a[1]);
    if (sorted.length > 1 && sorted[1][1] * 2 > sorted[0][1]) return { pkg: null, hit: false, conflict: true };
    if (sorted[0][1] < minHits) return { pkg: null, hit: false };
    return { pkg: sorted[0][0], hit: true };
  });
  // smoothing: a run of <= gap unlabelled statements between two hits of the SAME package takes that package
  const out = lab.map(x => ({ ...x }));
  let last = -1;
  for (let i = 0; i < lab.length; i++) {
    if (!lab[i].hit) continue;
    if (last >= 0 && lab[last].pkg === lab[i].pkg && i - last - 1 <= gap && i - last > 1) {
      let clean = true; for (let j = last + 1; j < i; j++) if (lab[j].conflict) clean = false;
      if (clean) for (let j = last + 1; j < i; j++) out[j] = { pkg: lab[i].pkg, hit: false, filled: true };
    }
    last = i;
  }
  return out;
}
// group a unit list into module-level statements with their nested units
export function statementsOf(us) {
  const st = us.filter(u => u.type === 'stmt').sort((a, b) => a.start - b.start).map(s => ({ ...s, units: [s] }));
  const nested = us.filter(u => u.type !== 'stmt' && u.type !== 'root').sort((a, b) => a.start - b.start);
  let j = 0;
  for (const u of nested) {
    while (j < st.length && st[j].end <= u.start) j++;
    if (j < st.length && st[j].start <= u.start && u.end <= st[j].end) st[j].units.push(u);
  }
  return st;
}
