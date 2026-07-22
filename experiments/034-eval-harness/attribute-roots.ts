/**
 * Noise root attribution: turn the eval's noise diff into a ranked worklist.
 *
 * For each noise statement (hash in both versions, text differs), token-diff
 * it against its prior twin and resolve every differing descriptive token to
 * the statement that DECLARES it on its own side. A token declared inside
 * the noise statement itself is an INTERNAL flip (close-match local churn);
 * a token declared elsewhere is an ECHO of a root binding whose name
 * flipped. Rank roots by echo fan-out and classify each root's declaring
 * statement (novel = changed-leaf chain, noise = flipped root, clean =
 * consistency artifact) so each fix goes to the root's tier, not the echoes.
 *
 *   npx tsx attribute-roots.ts <fresh.js> <prior.js> [topN]
 */
import * as fs from "node:fs";
import { statementsOf } from "./statements.js";

const maskPrivate = (t: string) => t.replace(/#[A-Za-z_$][\w$]*/g, "#_");
const words = (t: string) => t.match(/[A-Za-z_$][\w$]*/g) ?? [];
const isMintish = (w: string) =>
  w.length <= 4 || /^[A-Za-z]?[\w]?\d+_?$/.test(w) || /[_$]\d*$/.test(w);

function counted(tokens: string[]): Map<string, number> {
  const m = new Map<string, number>();
  for (const w of tokens) m.set(w, (m.get(w) ?? 0) + 1);
  return m;
}

/** Tokens whose occurrence count differs between the two texts. */
function diffTokens(a: string, b: string): Set<string> {
  const ca = counted(words(a));
  const cb = counted(words(b));
  const out = new Set<string>();
  for (const [w, n] of ca) if ((cb.get(w) ?? 0) !== n) out.add(w);
  for (const [w, n] of cb) if ((ca.get(w) ?? 0) !== n) out.add(w);
  return out;
}

/** Very light declaration scan: names introduced by decl keywords/params.
 * Heuristic on purpose — attribution needs the declaring STATEMENT, and a
 * name's first `var|let|const|function|class` site inside a statement is
 * that statement for wrapper-level code. */
function declaredNames(text: string): Set<string> {
  const out = new Set<string>();
  const re =
    /(?:\b(?:var|let|const)\s+|(?:^|\s)function\s+|(?:^|\s)class\s+)([A-Za-z_$][\w$]*)/g;
  for (const m of text.matchAll(re)) out.add(m[1]);
  return out;
}

interface RootInfo {
  token: string;
  echoStatements: number;
  echoLines: number;
  declStatus: string;
  declHead: string;
  pairedWith: Map<string, number>;
}

function main() {
  const [freshPath, priorPath, topNArg] = process.argv.slice(2);
  const topN = Number(topNArg ?? 30);
  const fresh = statementsOf(fs.readFileSync(freshPath, "utf8"));
  const prior = statementsOf(fs.readFileSync(priorPath, "utf8"));

  const priorByHash = new Map<string, { text: string }[]>();
  for (const s of prior) {
    const list = priorByHash.get(s.hash) ?? [];
    if (list.length === 0) priorByHash.set(s.hash, list);
    list.push(s);
  }
  const priorHashes = new Set(prior.map((s) => s.hash));

  // Declaring statement per name, per side (first declaration wins).
  const freshDecl = new Map<string, number>();
  fresh.forEach((s, i) => {
    for (const n of declaredNames(s.text)) {
      if (!freshDecl.has(n)) freshDecl.set(n, i);
    }
  });
  const priorDecl = new Map<string, number>();
  prior.forEach((s, i) => {
    for (const n of declaredNames(s.text)) {
      if (!priorDecl.has(n)) priorDecl.set(n, i);
    }
  });

  // Status of each fresh statement for root classification.
  const freshStatus = fresh.map((s) => {
    const twins = priorByHash.get(s.hash);
    if (!twins) return "novel";
    return twins.some((t) => t.text === s.text) ? "clean" : "noise";
  });

  const roots = new Map<string, RootInfo>();
  const internal = { statements: 0, lines: 0, tokens: new Set<string>() };
  let noiseSt = 0;

  const freshHashCounts = new Map<string, number>();
  for (const s of fresh) {
    freshHashCounts.set(s.hash, (freshHashCounts.get(s.hash) ?? 0) + 1);
  }
  let familyBucketSt = 0;
  let familyBucketLn = 0;
  fresh.forEach((s, idx) => {
    const twins = priorByHash.get(s.hash);
    if (!twins || twins.some((t) => t.text === s.text)) return;
    noiseSt++;
    // Attribution is only meaningful against THE twin — family buckets
    // (non-unique hashes) would diff arbitrary members and fabricate
    // phantom co-flip pairs. Count them separately.
    if (twins.length !== 1 || freshHashCounts.get(s.hash) !== 1) {
      familyBucketSt++;
      familyBucketLn += s.lines;
      return;
    }
    const twin = twins[0];
    const diff = diffTokens(maskPrivate(s.text), maskPrivate(twin.text));
    const descriptive = [...diff].filter((w) => !isMintish(w));
    if (descriptive.length === 0) return;

    const freshSide = descriptive.filter((w) => words(s.text).includes(w));
    const priorSide = descriptive.filter((w) => words(twin.text).includes(w));
    let sawInternal = false;
    for (const token of descriptive) {
      const declIdx = freshDecl.get(token) ?? priorDecl.get(token);
      const onFresh = freshDecl.has(token);
      const declInSelf = onFresh && freshDecl.get(token) === idx;
      if (declInSelf) {
        sawInternal = true;
        internal.tokens.add(token);
        continue;
      }
      let info = roots.get(token);
      if (!info) {
        const declStatus =
          declIdx === undefined
            ? "undeclared"
            : onFresh
              ? freshStatus[declIdx]
              : "prior-only";
        const declHead =
          declIdx !== undefined
            ? (onFresh ? fresh : prior)[declIdx].text
                .split("\n", 1)[0]
                .slice(0, 70)
            : "";
        info = {
          token,
          echoStatements: 0,
          echoLines: 0,
          declStatus,
          declHead,
          pairedWith: new Map()
        };
        roots.set(token, info);
      }
      info.echoStatements++;
      info.echoLines += s.lines;
      // co-flip pairing: fresh-side tokens pair with prior-side tokens
      const others = onFresh ? priorSide : freshSide;
      for (const o of others) {
        if (o !== token) {
          info.pairedWith.set(o, (info.pairedWith.get(o) ?? 0) + 1);
        }
      }
    }
    if (sawInternal) {
      internal.statements++;
      internal.lines += s.lines;
    }
  });

  const ranked = [...roots.values()].sort((a, b) => b.echoLines - a.echoLines);
  console.log(
    `noise statements: ${noiseSt} (${familyBucketSt} in family buckets / ${familyBucketLn} ln — not attributed); ` +
      `internal-flip statements: ${internal.statements} (${internal.lines} ln, ${internal.tokens.size} local tokens)`
  );
  console.log(`echo roots: ${roots.size}\n`);
  console.log("top roots by echoed lines:");
  for (const r of ranked.slice(0, topN)) {
    const pair = [...r.pairedWith.entries()].sort((a, b) => b[1] - a[1])[0];
    console.log(
      `  ${String(r.echoLines).padStart(6)} ln  ${String(r.echoStatements).padStart(3)} st  [${r.declStatus.padEnd(10)}] ${r.token}` +
        (pair ? `  ↔ ${pair[0]}` : "") +
        (r.declHead ? `\n           decl: ${r.declHead}` : "")
    );
  }
  // Ceiling B (consumer-set pairing for changed leaves): a NOVEL fresh
  // root and a PRIOR-ONLY root that co-flip reciprocally and uniquely
  // across the same twin statements are consumer-corroborated
  // counterparts — the echoes are the caller-set evidence. Count the
  // pairs a deterministic inherit could pin and the echo lines cleared.
  const topPartner = (r: RootInfo): [string, number] | undefined =>
    [...r.pairedWith.entries()].sort((a, b) => b[1] - a[1])[0];
  let pinnablePairs = 0;
  let pinnableLines = 0;
  const pinnable: string[] = [];
  for (const r of ranked) {
    if (r.declStatus !== "novel") continue;
    const partner = topPartner(r);
    if (!partner || partner[1] < 2) continue;
    const other = roots.get(partner[0]);
    if (!other || other.declStatus !== "prior-only") continue;
    const back = topPartner(other);
    if (!back || back[0] !== r.token) continue;
    pinnablePairs++;
    pinnableLines += r.echoLines;
    if (pinnable.length < 8) {
      pinnable.push(
        `${r.token} <= ${other.token} (${partner[1]} witnesses, ${r.echoLines} ln)`
      );
    }
  }
  console.log(
    `
ceiling B — reciprocal-unique changed-leaf pairs: ${pinnablePairs} pairs, ~${pinnableLines} echoed ln`
  );
  for (const line of pinnable) console.log(`  ${line}`);

  const byStatus = new Map<string, { roots: number; lines: number }>();
  for (const r of ranked) {
    const e = byStatus.get(r.declStatus) ?? { roots: 0, lines: 0 };
    e.roots++;
    e.lines += r.echoLines;
    byStatus.set(r.declStatus, e);
  }
  console.log(
    "\nroots by declaring-statement status (echoLines double-count shared statements):"
  );
  for (const [status, e] of byStatus) {
    console.log(
      `  ${status.padEnd(11)} ${String(e.roots).padStart(5)} roots  ${e.lines} echoed ln`
    );
  }
}

main();
