/**
 * Vendor churn, decomposed into real dependency change vs reducible noise.
 *
 * The eval scored `src/` only for its whole life (`run.sh` passed
 * `("$OUT/src" "$PRIOR_SRC")` and nothing else), so `vendor/` — 36,201 changed
 * lines across the four gate hops, 2.4x the entire measured `src` noise — was
 * never counted. This is the scorer that makes it a surface, used by
 * `analyze.ts` (per-pair KPI) and by `vendor-churn-decompose.ts` (the offline
 * report).
 *
 * ## THE UNIT IS THE FILE
 *
 * A vendored library is one to four lines of ~100KB minified text, so a
 * changed-line count says only that a file changed at all. Every bucket below
 * counts files; GNU-diff line counts ride alongside so the numbers reconcile
 * against the published baseline rather than approximating it.
 *
 * ## The predicate, stated so it can be checked against the claim (rule 3)
 *
 * `contentSignature` serializes a whole Program with
 *   - every BINDING replaced by a per-binding slot ordinal,
 *   - every literal, property key, operator and free identifier VERBATIM,
 *   - EXCEPT intra-tree `require("./…")` paths, which are masked.
 *
 * Two files with the same signature therefore differ only in the spelling of
 * their local bindings and in which humanify-chosen path they import.
 *
 * Masking intra-tree requires separates two noise sources that otherwise
 * contaminate each other: when humanify renames a vendor file, every file
 * requiring it changes text without any library changing (16 files on 85→86,
 * all of the form `S="../lodash/lib_eb5345cb.js" -> S="…-2.js"`). Bare
 * requires (`require("https")`) are NOT masked — those are real API surface.
 *
 * This is deliberately NOT `structuralHash`, the manifest's cross-version join
 * key, which passes `preserveLiterals: false` and so keeps only a string's
 * LENGTH and a number's order-of-magnitude bucket. `hash-probe.ts` shows six
 * of twelve semantic differences are invisible to it, which is why body reuse
 * must not be keyed on it — see this experiment's RESULTS.md.
 *
 * ## Matching is by CONTENT, not by path
 *
 * Vendor filenames are humanify's own output and they rotate: on 197→198 the
 * grammar under `kotlin.js` became WebAssembly's while the libraries were
 * untouched. Keying on path charges that rotation as added+removed files.
 * Grouping by content signature separates a library that genuinely arrived
 * from one that merely moved — the same trap as the leaderboard's name-keyed
 * `reloc` column (rule 7).
 */
import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import * as t from "@babel/types";
import {
  parseSourceAst,
  traverse,
  clearBabelTraverseCache
} from "../../src/babel-utils.js";
import { serializePathTokens } from "../../src/analysis/structural-hash.js";
import type { NodePath } from "@babel/traverse";

/** Sidecar metadata file inside vendor/, scored separately from library code. */
export const BUN_MANIFEST = "_bun-modules.json";

export interface Bucket {
  files: number;
  lines: number;
}

export interface VendorChurn {
  bodies: {
    filesPrior: number;
    filesFresh: number;
    /** Byte-identical at the same path. */
    identical: Bucket;
    /** Same path, same content signature — pure local-name reroll. */
    nameOnly: Bucket;
    /** Same path, diverging only in free minified tokens the minifier rerolled. */
    freeReroll: Bucket;
    /** This content exists on both sides, at a different path. */
    movedPath: Bucket;
    /** Same path, genuine content change. */
    realChange: Bucket;
    /** Content present on the fresh side only. */
    trulyAdded: Bucket;
    /** Content present on the prior side only. */
    trulyRemoved: Bucket;
    unparsed: Bucket;
    changedLinesTotal: number;
  };
  manifest: {
    changedLines: number;
    byField: Record<string, number>;
  };
  /** Total vendor churn in GNU-diff lines — the gate's headline number. */
  vendorTotalLines: number;
  /** Everything that changed without any library changing. Drive to zero. */
  noiseLines: number;
  /** Genuine dependency movement. Must NOT be driven down. */
  realDependencyChangeLines: number;
  realChangeFiles: string[];
  examples: {
    realChangeTop: Array<{ file: string; lines: number }>;
    movedPath: string[];
    trulyAdded: string[];
  };
}

const mk = (): Bucket => ({ files: 0, lines: 0 });
const bump = (b: Bucket, lines: number) => {
  b.files++;
  b.lines += lines;
};

/** Intra-tree require target — humanify's own layout, not library content. */
function isIntraTreePath(value: string): boolean {
  return value.startsWith("./") || value.startsWith("../");
}

interface FileAnalysis {
  sig: string;
  declared: Set<string>;
  toks: string[];
}

function analyzeFile(file: string): FileAnalysis | null {
  let code: string;
  try {
    code = fs.readFileSync(file, "utf-8");
  } catch {
    return null;
  }
  try {
    const ast = parseSourceAst(code);
    if (!ast) return null;
    let out: FileAnalysis | null = null;
    traverse(ast, {
      Program(p: NodePath) {
        p.traverse({
          CallExpression(c: NodePath<t.CallExpression>) {
            const callee = c.node.callee;
            const arg = c.node.arguments[0];
            if (
              t.isIdentifier(callee) &&
              callee.name === "require" &&
              t.isStringLiteral(arg) &&
              isIntraTreePath(arg.value)
            ) {
              arg.value = "<intra-tree-require>";
            }
          }
        });
        const declared = new Set<string>();
        p.traverse({
          Scopable(sp: NodePath) {
            for (const n of Object.keys(sp.scope?.bindings ?? {}))
              declared.add(n);
          }
        });
        for (const n of Object.keys(p.scope?.bindings ?? {})) declared.add(n);
        const toks = serializePathTokens(p, { preserveLiterals: true });
        out = {
          sig: createHash("sha256").update(toks.join(" ")).digest("hex"),
          declared,
          toks
        };
        p.stop();
      }
    });
    return out;
  } catch {
    return null;
  }
}

/** Bun/minifier token shape: short, no separators. */
const MINIFIED = /^[A-Za-z_$][A-Za-z0-9_$]{0,4}$/;

/**
 * Why two same-path files diverge. The serializer's `I=` class covers BOTH
 * non-computed property keys and free (unresolved) identifiers, so a moved
 * signature does not by itself mean the library changed: a bundle-level token
 * the minifier rerolled reads identically. Only a free, minified-shaped name
 * declared in NEITHER file is treated as reroll.
 */
function divergenceClass(
  a: FileAnalysis,
  b: FileAnalysis
): "free-minified-reroll" | "content" {
  if (a.toks.length !== b.toks.length) return "content";
  let sawDiff = false;
  for (let i = 0; i < a.toks.length; i++) {
    if (a.toks[i] === b.toks[i]) continue;
    sawDiff = true;
    const na = /^I=(.+)$/.exec(a.toks[i])?.[1];
    const nb = /^I=(.+)$/.exec(b.toks[i])?.[1];
    if (
      na == null ||
      nb == null ||
      !MINIFIED.test(na) ||
      !MINIFIED.test(nb) ||
      a.declared.has(na) ||
      b.declared.has(nb)
    ) {
      return "content";
    }
  }
  return sawDiff ? "free-minified-reroll" : "content";
}

function listJsFiles(root: string): string[] {
  const out: string[] = [];
  const walk = (dir: string) => {
    for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
      const full = path.join(dir, e.name);
      if (e.isDirectory()) walk(full);
      else if (e.name.endsWith(".js")) out.push(path.relative(root, full));
    }
  };
  walk(root);
  return out.sort();
}

/**
 * GNU-diff changed-line count — the SAME tool and unit as the published
 * baseline, so results reconcile with 36,201 instead of approximating it. A
 * position-blind multiset count was tried first and ran systematically low
 * (8,841 → 6,924 on 215→216) because a line that merely MOVES cancels against
 * its own copy on the other side.
 */
function changedLines(fileA: string, fileB: string): number {
  const r = spawnSync("diff", [fileA, fileB], {
    encoding: "utf-8",
    maxBuffer: 1 << 30
  });
  let n = 0;
  for (const line of (r.stdout ?? "").split("\n")) {
    if (line.startsWith("<") || line.startsWith(">")) n++;
  }
  return n;
}

const lineCount = (file: string): number =>
  fs.readFileSync(file, "utf-8").split("\n").length;

function decomposeManifest(
  priorDir: string,
  freshDir: string
): { changedLines: number; byField: Record<string, number> } {
  const mp = path.join(priorDir, BUN_MANIFEST);
  const mf = path.join(freshDir, BUN_MANIFEST);
  if (!fs.existsSync(mp) || !fs.existsSync(mf)) {
    return { changedLines: 0, byField: {} };
  }
  const byField = new Map<string, number>();
  const r = spawnSync("diff", [mp, mf], {
    encoding: "utf-8",
    maxBuffer: 1 << 30
  });
  let total = 0;
  for (const line of (r.stdout ?? "").split("\n")) {
    if (!line.startsWith("<") && !line.startsWith(">")) continue;
    total++;
    const key = /^\s*"([^"]+)":/.exec(line.slice(1))?.[1] ?? "(structural)";
    byField.set(key, (byField.get(key) ?? 0) + 1);
  }
  return {
    changedLines: total,
    byField: Object.fromEntries(
      [...byField.entries()].sort((x, y) => y[1] - x[1])
    )
  };
}

export function decomposeVendorChurn(
  priorDir: string,
  freshDir: string
): VendorChurn {
  const priorRel = listJsFiles(priorDir);
  const freshRel = listJsFiles(freshDir);

  const sigOf = new Map<string, string>();
  const infoOf = new Map<string, FileAnalysis>();
  for (const [side, dir, rels] of [
    ["prior", priorDir, priorRel],
    ["fresh", freshDir, freshRel]
  ] as const) {
    for (const rel of rels) {
      const a = analyzeFile(path.join(dir, rel));
      clearBabelTraverseCache();
      if (!a) continue;
      sigOf.set(`${side}|${rel}`, a.sig);
      infoOf.set(`${side}|${rel}`, a);
    }
  }

  const priorBySig = new Map<string, string[]>();
  for (const rel of priorRel) {
    const s = sigOf.get(`prior|${rel}`);
    if (s) priorBySig.set(s, [...(priorBySig.get(s) ?? []), rel]);
  }
  const freshBySig = new Map<string, string[]>();
  for (const rel of freshRel) {
    const s = sigOf.get(`fresh|${rel}`);
    if (s) freshBySig.set(s, [...(freshBySig.get(s) ?? []), rel]);
  }

  const identical = mk();
  const nameOnly = mk();
  const freeReroll = mk();
  const movedPath = mk();
  const realChange = mk();
  const trulyAdded = mk();
  const trulyRemoved = mk();
  const unparsed = mk();

  const realChangeFiles: Array<{ file: string; lines: number }> = [];
  const movedExamples: string[] = [];
  const addedExamples: string[] = [];

  const priorSet = new Set(priorRel);
  for (const rel of freshRel) {
    const fp = path.join(freshDir, rel);
    const fsig = sigOf.get(`fresh|${rel}`);
    if (fsig === undefined) {
      bump(unparsed, lineCount(fp));
      continue;
    }

    if (priorSet.has(rel)) {
      const pp = path.join(priorDir, rel);
      if (fs.readFileSync(pp, "utf-8") === fs.readFileSync(fp, "utf-8")) {
        identical.files++;
        continue;
      }
      const lines = changedLines(pp, fp);
      if (sigOf.get(`prior|${rel}`) === fsig) {
        bump(nameOnly, lines);
        continue;
      }
      const a = infoOf.get(`prior|${rel}`);
      const b = infoOf.get(`fresh|${rel}`);
      if (a && b && divergenceClass(a, b) === "free-minified-reroll") {
        bump(freeReroll, lines);
        continue;
      }
      // Same path, different content — but the content may simply have SWAPPED
      // places with another file. Charge real change only when this exact
      // content is absent from the prior tree entirely.
      if (priorBySig.has(fsig)) {
        bump(movedPath, lines);
        if (movedExamples.length < 8) {
          movedExamples.push(`${rel} <- ${priorBySig.get(fsig)?.[0]}`);
        }
        continue;
      }
      bump(realChange, lines);
      realChangeFiles.push({ file: rel, lines });
      continue;
    }

    if (priorBySig.has(fsig)) {
      bump(movedPath, lineCount(fp));
      if (movedExamples.length < 8) {
        movedExamples.push(`${rel} <- ${priorBySig.get(fsig)?.[0]}`);
      }
    } else {
      bump(trulyAdded, lineCount(fp));
      if (addedExamples.length < 8) addedExamples.push(rel);
    }
  }

  const freshSet = new Set(freshRel);
  for (const rel of priorRel) {
    if (freshSet.has(rel)) continue;
    const psig = sigOf.get(`prior|${rel}`);
    if (psig !== undefined && freshBySig.has(psig)) continue; // counted as moved
    bump(trulyRemoved, lineCount(path.join(priorDir, rel)));
  }

  const manifest = decomposeManifest(priorDir, freshDir);
  const bodyChanged =
    nameOnly.lines +
    freeReroll.lines +
    realChange.lines +
    movedPath.lines +
    unparsed.lines;

  return {
    bodies: {
      filesPrior: priorRel.length,
      filesFresh: freshRel.length,
      identical,
      nameOnly,
      freeReroll,
      movedPath,
      realChange,
      trulyAdded,
      trulyRemoved,
      unparsed,
      changedLinesTotal: bodyChanged
    },
    manifest,
    vendorTotalLines:
      bodyChanged +
      manifest.changedLines +
      trulyAdded.lines +
      trulyRemoved.lines,
    noiseLines:
      nameOnly.lines +
      freeReroll.lines +
      movedPath.lines +
      manifest.changedLines,
    realDependencyChangeLines:
      realChange.lines + trulyAdded.lines + trulyRemoved.lines,
    realChangeFiles: realChangeFiles.map((e) => e.file),
    examples: {
      realChangeTop: realChangeFiles
        .sort((x, y) => y.lines - x.lines)
        .slice(0, 15),
      movedPath: movedExamples,
      trulyAdded: addedExamples
    }
  };
}
