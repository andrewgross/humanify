#!/usr/bin/env -S npx tsx
/**
 * exp091 task 0 — would a prior-file avoid-list help or hurt?
 *
 * Counts, on one real hop (prior tree, fresh tree, the fresh run's -vv log):
 *
 *  (1) COLLISIONS the list would prevent: a name-only rename old->new where
 *      `new` was a DIFFERENT binding's name in the prior file, owned by a
 *      binding in another top-level function (so the design's avoid set
 *      would have contained it and validation would have refused it).
 *
 *  (2) CORRECT REUSES the list would block: an LLM answer equal to the SAME
 *      binding's prior name (same file, masked-identical declaration line)
 *      where the prompt never showed that name — the correspondence
 *      machinery missed it, the model landed it anyway. With no match, the
 *      pipeline sees the name as live-elsewhere in the prior file and the
 *      list would have refused it.
 *
 * Units: changed lines (1) vs lines that would newly change (2, every
 * occurrence of the name in the fresh file), plus distinct bindings.
 *
 * WRONGLY includes / misses (stated before quoting numbers):
 *  - (1) pairs come from masked-line positional pairing (rename-pairs.py's
 *    method): lines that ALSO changed structurally are missed; multiset
 *    pairing can pair unrelated lines with the same mask (over-count).
 *  - (1) uses the prior binding named `old`; when several prior bindings
 *    share that name the owner function is ambiguous -> bucketed apart.
 *  - (2) maps an answer to a fresh binding by NAME; answers whose name is
 *    declared in several fresh files are ambiguous -> bucketed apart.
 *  - (2) "not in prompt" is a plain substring test over the prompt text.
 */
import fs from "node:fs";
import path from "node:path";
import { parse } from "@babel/parser";
import traverseModule from "@babel/traverse";
const traverse: typeof traverseModule =
  (traverseModule as unknown as { default?: typeof traverseModule }).default ??
  traverseModule;

const [priorRoot, freshRoot, logPath, outPath] = process.argv.slice(2);
if (!priorRoot || !freshRoot || !logPath || !outPath) {
  console.error(
    "usage: task0-census.ts <prior/src> <fresh/src> <fresh -vv log> <out.json>"
  );
  process.exit(2);
}

// ---------- masking (port of 083/rename-pairs.py) ----------
const KW = new Set(
  `var let const function return if else for while do switch case break continue
new typeof instanceof in of delete void null true false this async await yield throw try
catch finally class extends super import export from default require module exports
static get set`.split(/\s+/)
);
const TOKEN = /(\.\s*)?([A-Za-z_$][A-Za-z0-9_$]*)(\s*:)?/g;
function maskAndNames(line: string): { mask: string; names: string[] } {
  const names: string[] = [];
  const mask = line
    .replace(TOKEN, (m, dot, name, colon) => {
      if (dot || colon || KW.has(name)) return m;
      names.push(name);
      return `${dot ?? ""}X${colon ?? ""}`;
    })
    .trim();
  return { mask, names };
}

// ---------- binding index ----------
interface BindingEntry {
  name: string;
  fnKey: string; // enclosing top-level statement key ("stmt:<index>")
  line: number;
  declMask: string;
  /** masked text of the enclosing top-level statement — the strict "same binding" identity */
  stmtMask: string;
}
interface FileIndex {
  bindings: Map<string, BindingEntry[]>;
  lines: string[];
}
function listJs(root: string): string[] {
  const out: string[] = [];
  const walk = (d: string) => {
    for (const e of fs.readdirSync(d, { withFileTypes: true })) {
      const p = path.join(d, e.name);
      if (e.isDirectory()) walk(p);
      else if (e.name.endsWith(".js")) out.push(path.relative(root, p));
    }
  };
  walk(root);
  return out;
}
function indexFile(file: string): FileIndex | null {
  const code = fs.readFileSync(file, "utf8");
  const lines = code.split("\n");
  let ast: ReturnType<typeof parse>;
  try {
    ast = parse(code, {
      sourceType: "unambiguous",
      errorRecovery: true,
      plugins: ["jsx"]
    });
  } catch {
    return null;
  }
  const bindings = new Map<string, BindingEntry[]>();
  // top-level statement index by line range
  const stmts = ast.program.body.map((s, i) => ({
    i,
    start: s.loc?.start.line ?? 0,
    end: s.loc?.end.line ?? 0
  }));
  const stmtMasks = stmts.map((t) =>
    lines
      .slice(t.start - 1, t.end)
      .map((l) => maskAndNames(l).mask)
      .join("\n")
  );
  const stmtFor = (line: number) =>
    stmts.find((t) => line >= t.start && line <= t.end);
  const fnKeyFor = (line: number): string => {
    const s = stmtFor(line);
    return s ? `stmt:${s.i}` : "stmt:?";
  };
  // biome-ignore lint/suspicious/noExplicitAny: CJS/ESM default-export shape differs between tsx and tsc
  (traverse as any)(ast, {
    Scope(p: any) {
      for (const [name, b] of Object.entries(p.scope.bindings) as [
        string,
        any
      ][]) {
        const line = b.identifier.loc?.start.line ?? 0;
        const entry: BindingEntry = {
          name,
          fnKey: fnKeyFor(line),
          line,
          declMask: maskAndNames(lines[line - 1] ?? "").mask,
          stmtMask: stmtMasks[stmtFor(line)?.i ?? -1] ?? ""
        };
        const arr = bindings.get(name) ?? [];
        // Babel visits each scope once; guard against duplicate registration
        if (!arr.some((e) => e.line === line)) arr.push(entry);
        bindings.set(name, arr);
      }
    }
  });
  return { bindings, lines };
}

// ---------- rename pairs per file (port of rename-pairs.py) ----------
interface Pair {
  file: string;
  old: string;
  new: string;
  n: number;
}
function renamePairs(rel: string, prior: string[], fresh: string[]): Pair[] {
  const count = (ls: string[]) => {
    const m = new Map<string, number>();
    for (const l of ls) m.set(l, (m.get(l) ?? 0) + 1);
    return m;
  };
  const pc = count(prior);
  const fc = count(fresh);
  const gone: string[] = [];
  const came: string[] = [];
  for (const [l, n] of pc)
    for (let i = 0; i < n - (fc.get(l) ?? 0); i++) gone.push(l);
  for (const [l, n] of fc)
    for (let i = 0; i < n - (pc.get(l) ?? 0); i++) came.push(l);
  const byMask = new Map<string, { g: string[][]; c: string[][] }>();
  for (const l of gone) {
    const { mask, names } = maskAndNames(l);
    const e = byMask.get(mask) ?? { g: [], c: [] };
    e.g.push(names);
    byMask.set(mask, e);
  }
  for (const l of came) {
    const { mask, names } = maskAndNames(l);
    const e = byMask.get(mask) ?? { g: [], c: [] };
    e.c.push(names);
    byMask.set(mask, e);
  }
  const pairs = new Map<string, Pair>();
  const key = (a: string[]) => a.join("\u0000");
  for (const { g, c } of byMask.values()) {
    const gs = [...g].sort((a, b) => key(a).localeCompare(key(b)));
    const cs = [...c].sort((a, b) => key(a).localeCompare(key(b)));
    for (let i = 0; i < Math.min(gs.length, cs.length); i++) {
      if (gs[i].length !== cs[i].length) continue;
      for (let j = 0; j < gs[i].length; j++) {
        if (gs[i][j] === cs[i][j]) continue;
        const k = `${gs[i][j]}\u0000${cs[i][j]}`;
        const p = pairs.get(k) ?? {
          file: rel,
          old: gs[i][j],
          new: cs[i][j],
          n: 0
        };
        p.n++;
        pairs.set(k, p);
      }
    }
  }
  return [...pairs.values()];
}

// ---------- log answers ----------
interface Answer {
  kind: string;
  prompt: string;
  renames: Record<string, string>;
}
function readAnswers(log: string): Answer[] {
  const content = fs.readFileSync(log, { encoding: "utf8" });
  const out: Answer[] = [];
  for (const b of content.split("=".repeat(80))) {
    if (!b.includes("suggestAllNames - SUCCESS")) continue;
    if (!b.includes("--- USER PROMPT ---") || !b.includes("--- PARSED ---"))
      continue;
    if (b.includes("Identifiers: lib_")) continue;
    const up = b.split("--- USER PROMPT ---", 2)[1];
    const prompt = up.split("\n--- ", 2)[0];
    const parsedTxt = b.split("--- PARSED ---", 2)[1].trim();
    let obj: unknown;
    try {
      obj = JSON.parse(parsedTxt.slice(0, parsedTxt.lastIndexOf("}") + 1));
    } catch {
      continue;
    }
    if (!obj || typeof obj !== "object") continue;
    const raw = ((obj as Record<string, unknown>).renames ?? obj) as Record<
      string,
      unknown
    >;
    const renames: Record<string, string> = {};
    for (const [k, v] of Object.entries(raw))
      if (typeof v === "string" && !k.startsWith("_")) renames[k] = v;
    let kind = "function-fresh";
    if (prompt.includes("Please suggest DIFFERENT names")) kind = "retry";
    else if (
      prompt.includes("A prior version of this function was already named")
    )
      kind = "function-closematch";
    else if (prompt.includes("Analyze these top-level module identifiers"))
      kind = prompt.includes("Prior version name:")
        ? "module-level+hint"
        : "module-level";
    else if (prompt.startsWith("\nCode:\n") || prompt.includes("\nIs retry: "))
      kind = "function-compact";
    else if (!prompt.includes("Analyze this function")) kind = "other";
    out.push({ kind, prompt, renames });
  }
  return out;
}

// ---------- main ----------
const priorFiles = new Set(listJs(priorRoot));
const freshFiles = listJs(freshRoot);
const shared = freshFiles.filter((f) => priorFiles.has(f));
const priorIdx = new Map<string, FileIndex>();
const freshIdx = new Map<string, FileIndex>();
let unparsed = 0;
for (const rel of shared) {
  const p = indexFile(path.join(priorRoot, rel));
  const f = indexFile(path.join(freshRoot, rel));
  if (!p || !f) {
    unparsed++;
    continue;
  }
  priorIdx.set(rel, p);
  freshIdx.set(rel, f);
}

const answers = readAnswers(logPath);
/** every name the model produced this run -> the prompt kinds that produced it */
const llmNameKinds = new Map<string, Set<string>>();
for (const a of answers)
  for (const v of Object.values(a.renames)) {
    const s = llmNameKinds.get(v) ?? new Set();
    s.add(a.kind);
    llmNameKinds.set(v, s);
  }

// (1) collisions
const c1 = {
  pairsSeen: 0,
  linesSeen: 0,
  oldNotPriorBinding: { pairs: 0, lines: 0 },
  newNotPriorBinding: { pairs: 0, lines: 0 },
  oldAmbiguous: { pairs: 0, lines: 0 },
  sameFunctionOwner: { pairs: 0, lines: 0 },
  otherFunctionOwner: {
    pairs: 0,
    lines: 0,
    ownerKeptNameInFresh: 0,
    ownerRenamedInFresh: 0
  },
  otherFunctionOwnerNotLlm: { pairs: 0, lines: 0 },
  otherFunctionOwnerByKind: {} as Record<string, number>,
  samples: [] as string[]
};
for (const rel of priorIdx.keys()) {
  const P = priorIdx.get(rel)!;
  const F = freshIdx.get(rel)!;
  for (const pr of renamePairs(rel, P.lines, F.lines)) {
    c1.pairsSeen++;
    c1.linesSeen += pr.n;
    const olds = P.bindings.get(pr.old);
    const news = P.bindings.get(pr.new);
    if (!olds) {
      c1.oldNotPriorBinding.pairs++;
      c1.oldNotPriorBinding.lines += pr.n;
      continue;
    }
    if (!news) {
      c1.newNotPriorBinding.pairs++;
      c1.newNotPriorBinding.lines += pr.n;
      continue;
    }
    const oldFns = new Set(olds.map((e) => e.fnKey));
    if (oldFns.size > 1) {
      c1.oldAmbiguous.pairs++;
      c1.oldAmbiguous.lines += pr.n;
      continue;
    }
    const oldFn = olds[0].fnKey;
    const otherOwner = news.find((e) => e.fnKey !== oldFn);
    if (!otherOwner) {
      c1.sameFunctionOwner.pairs++;
      c1.sameFunctionOwner.lines += pr.n;
      continue;
    }
    const kinds = llmNameKinds.get(pr.new);
    if (!kinds) {
      // the name reached the tree by transfer/reconcile, not from a prompt —
      // a prompt-side avoid list cannot touch it
      c1.otherFunctionOwnerNotLlm.pairs++;
      c1.otherFunctionOwnerNotLlm.lines += pr.n;
      continue;
    }
    c1.otherFunctionOwner.pairs++;
    c1.otherFunctionOwner.lines += pr.n;
    for (const k of kinds)
      c1.otherFunctionOwnerByKind[k] =
        (c1.otherFunctionOwnerByKind[k] ?? 0) + 1;
    // did the prior owner keep its name in the fresh file (same masked statement)?
    const kept = (F.bindings.get(pr.new) ?? []).some(
      (e) => e.stmtMask === otherOwner.stmtMask && e.fnKey !== "stmt:?"
    );
    if (kept) c1.otherFunctionOwner.ownerKeptNameInFresh++;
    else c1.otherFunctionOwner.ownerRenamedInFresh++;
    if (c1.samples.length < 25)
      c1.samples.push(
        `${rel}: ${pr.old} -> ${pr.new} (${pr.n} ln; prior owner of '${pr.new}' at L${otherOwner.line}, ${kept ? "still named so in fresh" : "renamed in fresh"})`
      );
  }
}

// (2) correct reuses the list would block
const freshDeclFiles = new Map<string, Set<string>>();
for (const [rel, F] of freshIdx)
  for (const name of F.bindings.keys()) {
    const s = freshDeclFiles.get(name) ?? new Set();
    s.add(rel);
    freshDeclFiles.set(name, s);
  }
const c2 = {
  answersSeen: 0,
  byKind: {} as Record<string, number>,
  nameNotDeclaredInFresh: 0,
  nameAmbiguousAcrossFiles: 0,
  notPriorNameOfSameBinding: 0,
  duplicateAnswerSameBinding: 0,
  priorNameLanded: {
    strict: {
      total: 0,
      nameInPrompt: 0,
      unaided: 0,
      unaidedLines: 0,
      unaidedByKind: {} as Record<string, number>
    },
    loose: {
      total: 0,
      nameInPrompt: 0,
      unaided: 0,
      unaidedLines: 0,
      unaidedByKind: {} as Record<string, number>
    }
  },
  samples: [] as string[]
};
const seenBinding = new Set<string>();
const occLines = (F: FileIndex, name: string) => {
  const re = new RegExp(
    `(^|[^A-Za-z0-9_$])${name.replace(/\$/g, "\\$")}([^A-Za-z0-9_$]|$)`
  );
  return F.lines.filter((l) => re.test(l)).length;
};
for (const a of answers) {
  c2.byKind[a.kind] = (c2.byKind[a.kind] ?? 0) + 1;
  for (const [ident, name] of Object.entries(a.renames)) {
    c2.answersSeen++;
    const files = freshDeclFiles.get(name);
    if (!files) {
      c2.nameNotDeclaredInFresh++;
      continue;
    }
    if (files.size > 1) {
      c2.nameAmbiguousAcrossFiles++;
      continue;
    }
    const rel = [...files][0];
    const F = freshIdx.get(rel)!;
    const P = priorIdx.get(rel)!;
    const fe = F.bindings.get(name)!;
    const pe = P.bindings.get(name) ?? [];
    const strict = fe.some((f) => pe.some((p) => p.stmtMask === f.stmtMask));
    const loose = fe.some((f) => pe.some((p) => p.declMask === f.declMask));
    if (!loose) {
      c2.notPriorNameOfSameBinding++;
      continue;
    }
    const bkey = `${rel}\u0000${name}`;
    if (seenBinding.has(bkey)) {
      c2.duplicateAnswerSameBinding++;
      continue;
    }
    seenBinding.add(bkey);
    const inPrompt = new RegExp(
      `(^|[^A-Za-z0-9_$])${name.replace(/\$/g, "\\$")}([^A-Za-z0-9_$]|$)`
    ).test(a.prompt);
    for (const [tier, hit] of [
      ["loose", loose],
      ["strict", strict]
    ] as const) {
      if (!hit) continue;
      const t = c2.priorNameLanded[tier];
      t.total++;
      if (inPrompt) {
        t.nameInPrompt++;
        continue;
      }
      t.unaided++;
      t.unaidedLines += occLines(F, name);
      t.unaidedByKind[a.kind] = (t.unaidedByKind[a.kind] ?? 0) + 1;
    }
    if (strict && !inPrompt && c2.samples.length < 25)
      c2.samples.push(
        `${rel}: ${ident} -> ${name} [${a.kind}] (${occLines(F, name)} ln)`
      );
  }
}

const result = {
  inputs: {
    priorRoot,
    freshRoot,
    logPath,
    sharedFiles: shared.length,
    unparsed
  },
  collisionsListWouldPrevent: c1,
  correctReusesListWouldBlock: c2
};
fs.writeFileSync(outPath, JSON.stringify(result, null, 2));

const l = (o: { pairs: number; lines: number }) =>
  `${o.pairs} pairs / ${o.lines} ln`;
console.log(`files compared: ${shared.length} (unparsed ${unparsed})`);
console.log(`\n(1) COLLISIONS the avoid-list would prevent`);
console.log(`  rename pairs seen: ${c1.pairsSeen} / ${c1.linesSeen} ln`);
console.log(
  `  new name owned by another binding in ANOTHER prior function AND produced by a prompt this run: ${l(c1.otherFunctionOwner)}  <- the frameRows class, what the list can reach`
);
console.log(
  `     by prompt kind: ${JSON.stringify(c1.otherFunctionOwnerByKind)}`
);
console.log(
  `     owner still carries that name in fresh: ${c1.otherFunctionOwner.ownerKeptNameInFresh} pairs; owner itself renamed: ${c1.otherFunctionOwner.ownerRenamedInFresh}`
);
console.log(
  `  same, but the name came by transfer/reconcile, not a prompt (list can't reach): ${l(c1.otherFunctionOwnerNotLlm)}`
);
console.log(
  `  new name owned in the SAME prior function (existing dup check territory): ${l(c1.sameFunctionOwner)}`
);
console.log(
  `  old binding ambiguous (several prior bindings share it): ${l(c1.oldAmbiguous)}`
);
console.log(
  `  new name not a prior binding at all (genuinely fresh word): ${l(c1.newNotPriorBinding)}`
);
console.log(
  `  old not a prior binding (property/alias token): ${l(c1.oldNotPriorBinding)}`
);
console.log(`\n(2) CORRECT REUSES the avoid-list would block`);
console.log(
  `  LLM answers: ${c2.answersSeen} across ${answers.length} prompts ${JSON.stringify(c2.byKind)}`
);
for (const tier of ["strict", "loose"] as const) {
  const t = c2.priorNameLanded[tier];
  console.log(
    `  [${tier}: ${tier === "strict" ? "same masked STATEMENT" : "same masked decl LINE only (upper bound)"}] answer == prior name of the same binding: ${t.total} bindings`
  );
  console.log(
    `     name was in the prompt (hinted/prior block/context): ${t.nameInPrompt}`
  );
  console.log(
    `     UNAIDED (not in prompt) -> list would block: ${t.unaided} bindings / ${t.unaidedLines} ln  ${JSON.stringify(t.unaidedByKind)}`
  );
}
console.log(
  `  (excluded: name not declared in fresh ${c2.nameNotDeclaredInFresh}, ambiguous across files ${c2.nameAmbiguousAcrossFiles}, not the prior name ${c2.notPriorNameOfSameBinding}, duplicate answers for a binding already counted ${c2.duplicateAnswerSameBinding})`
);
console.log(`\nsamples (1):\n  ${c1.samples.join("\n  ")}`);
console.log(`\nsamples (2):\n  ${c2.samples.join("\n  ")}`);
