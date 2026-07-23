/**
 * Knock-on simulation for the HASH-TWIN CONSUMER PASS (standing next
 * step 1): compute reciprocal changed-leaf root pairs from hash-paired
 * twins — the consumer tier's evidence, but reorder-free, so shuffle
 * pairs are reachable — apply the root renames text-level, and measure
 * how much noise clears (direct + echo). Iterates: cleared echoes can
 * expose new reciprocal pairs.
 *
 * Pair gates per round (mirror the reconcile consumer tier):
 *   - co-flip witnessed in >=2 DISTINCT unique-twin noise statements
 *   - reciprocal-unique: fresh token's top partner is the prior token
 *     and vice versa, no other candidate pairing either token
 *   - fresh token NOVEL this hop (absent from prior text)
 *   - prior token DEAD in fresh text
 *   - neither token minted-looking (mint drift is a different lever)
 *
 *   npx tsx simulate-root-inherit.ts <fresh.js> <prior.js> [maxRounds]
 */
import * as fs from "node:fs";
import { statementsOf } from "./statements.js";

interface Stmt {
  hash: string;
  text: string;
  lines: number;
}

const WORD = /[A-Za-z_$][\w$]*/g;
const isMintish = (w: string) =>
  w.length <= 4 || /^[A-Za-z]?[\w]?\d+_?$/.test(w) || /[_$]\d*$/.test(w);

function counted(tokens: string[]): Map<string, number> {
  const m = new Map<string, number>();
  for (const w of tokens) m.set(w, (m.get(w) ?? 0) + 1);
  return m;
}

interface PairEvidence {
  witnesses: Set<number>;
  partners: Map<string, number>;
}

function main() {
  const [freshPath, priorPath, maxRoundsArg] = process.argv.slice(2);
  const maxRounds = Number(maxRoundsArg ?? 8);
  const priorText = fs.readFileSync(priorPath, "utf8");
  const fresh = statementsOf(fs.readFileSync(freshPath, "utf8")) as Stmt[];
  const prior = statementsOf(priorText) as Stmt[];
  const priorCensus = new Set(priorText.match(WORD) ?? []);

  const priorByHash = new Map<string, Stmt[]>();
  for (const s of prior) {
    const list = priorByHash.get(s.hash) ?? [];
    if (list.length === 0) priorByHash.set(s.hash, list);
    list.push(s);
  }
  const freshHashCount = new Map<string, number>();
  for (const s of fresh) {
    freshHashCount.set(s.hash, (freshHashCount.get(s.hash) ?? 0) + 1);
  }

  const noiseNow = (): { st: number; ln: number } => {
    let st = 0;
    let ln = 0;
    for (const s of fresh) {
      const twins = priorByHash.get(s.hash);
      if (!twins || twins.some((t) => t.text === s.text)) continue;
      st++;
      ln += s.lines;
    }
    return { st, ln };
  };

  const start = noiseNow();
  console.log(`start: ${start.st} noise st / ${start.ln} noise ln`);

  let totalPairs = 0;
  for (let round = 1; round <= maxRounds; round++) {
    const freshCensus = new Set<string>();
    for (const s of fresh) {
      for (const w of s.text.match(WORD) ?? []) freshCensus.add(w);
    }

    // Evidence: token-count diffs of unique-twin noise statements.
    const freshEvidence = new Map<string, PairEvidence>();
    const priorEvidence = new Map<string, PairEvidence>();
    fresh.forEach((s, idx) => {
      const twins = priorByHash.get(s.hash);
      if (!twins || twins.length !== 1) return;
      if (freshHashCount.get(s.hash) !== 1) return;
      const twin = twins[0];
      if (s.text === twin.text) return;
      const ca = counted(s.text.match(WORD) ?? []);
      const cb = counted(twin.text.match(WORD) ?? []);
      const freshSide: string[] = [];
      const priorSide: string[] = [];
      for (const [w, n] of ca) {
        if ((cb.get(w) ?? 0) !== n && !isMintish(w)) freshSide.push(w);
      }
      for (const [w, n] of cb) {
        if ((ca.get(w) ?? 0) !== n && !isMintish(w)) priorSide.push(w);
      }
      for (const f of freshSide) {
        let ev = freshEvidence.get(f);
        if (!ev) {
          ev = { witnesses: new Set(), partners: new Map() };
          freshEvidence.set(f, ev);
        }
        ev.witnesses.add(idx);
        for (const p of priorSide) {
          ev.partners.set(p, (ev.partners.get(p) ?? 0) + 1);
        }
      }
      for (const p of priorSide) {
        let ev = priorEvidence.get(p);
        if (!ev) {
          ev = { witnesses: new Set(), partners: new Map() };
          priorEvidence.set(p, ev);
        }
        ev.witnesses.add(idx);
        for (const f of freshSide) {
          ev.partners.set(f, (ev.partners.get(f) ?? 0) + 1);
        }
      }
    });

    const top = (ev: PairEvidence): [string, number] | undefined =>
      [...ev.partners.entries()].sort((a, b) => b[1] - a[1])[0];

    const mapping = new Map<string, string>();
    for (const [freshTok, ev] of freshEvidence) {
      if (priorCensus.has(freshTok)) continue; // not novel this hop
      if (ev.witnesses.size < 2) continue;
      const partner = top(ev);
      if (!partner || partner[1] < 2) continue;
      const [priorTok] = partner;
      if (freshCensus.has(priorTok)) continue; // prior name still live
      const back = priorEvidence.get(priorTok);
      if (!back) continue;
      const backTop = top(back);
      if (!backTop || backTop[0] !== freshTok) continue; // not reciprocal
      // Unique both directions: no OTHER fresh token's top partner is
      // this prior token (checked by claim map below).
      mapping.set(freshTok, priorTok);
    }
    // Injectivity: drop pairs whose target is claimed twice.
    const claims = new Map<string, number>();
    for (const to of mapping.values()) {
      claims.set(to, (claims.get(to) ?? 0) + 1);
    }
    for (const [from, to] of [...mapping]) {
      if ((claims.get(to) ?? 0) !== 1) mapping.delete(from);
    }

    if (mapping.size === 0) {
      console.log(`round ${round}: fixpoint (no reciprocal pairs)`);
      break;
    }
    for (const s of fresh) {
      s.text = s.text.replace(WORD, (w) => mapping.get(w) ?? w);
    }
    totalPairs += mapping.size;
    const now = noiseNow();
    const sample = [...mapping].slice(0, 3);
    console.log(
      `round ${round}: ${mapping.size} root pairs -> ${now.st} st / ${now.ln} ln  ` +
        `(e.g. ${sample.map(([f, p]) => `${f}<-${p}`).join(", ")})`
    );
  }

  const end = noiseNow();
  console.log(
    `\nresult: ${totalPairs} root pairs; noise ${start.st}->${end.st} st ` +
      `(${(((start.st - end.st) / start.st) * 100).toFixed(1)}%), ` +
      `${start.ln}->${end.ln} ln (${(((start.ln - end.ln) / start.ln) * 100).toFixed(1)}%)`
  );
}

main();
