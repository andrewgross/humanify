/**
 * Lever 2 ceiling: same-name relocation (`reloc`) recoverability.
 *
 * A reloc is a binding that kept its name but changed home file, dragging
 * every importer's require-alias. The candidate deterministic signal is
 * NEIGHBOR CONTEXT: if the declaring statement's previous/next statement
 * hashes map unanimously to one prior file — the file the name lived in —
 * a neighbor tier would have kept it there.
 *
 * For each pair, diff a fresh ledger against the prior archive ledger:
 *  - reloc names (both sides, different home file)
 *  - recoverable-strict: BOTH neighbors resolve unanimously to the prior
 *    home file
 *  - recoverable-either: at least one neighbor resolves to the prior home
 *    and the other does not contradict (null or same)
 *  - ownHashUnanimous: the statement's own hash already maps unanimously
 *    to the prior home (the existing hash tier should have caught it —
 *    diagnostic for why it didn't)
 *
 *   npx tsx ceiling-lever2.ts <freshRoot> ...  (freshRoot contains
 *     <ver>/.humanify/{humanified.js,split-ledger.json}; pairs from
 *     pairs.json; falls back to the archive for the fresh side too)
 */
import * as fs from "node:fs";
import * as path from "node:path";
import { parseSync } from "@babel/core";
import * as t from "@babel/types";
import { findWrapperFunction } from "../../src/analysis/wrapper-detection.js";

const HERE = path.dirname(new URL(import.meta.url).pathname);

interface Ledger {
  nameToFiles: Record<string, string[]>;
  order: string[];
  hashes?: string[];
}

/** Every binding identifier a top-level statement declares — variable
 * declarators (incl. destructuring patterns), fn/class ids. */
function declaredNamesOf(stmt: t.Statement): string[] {
  return Object.keys(t.getBindingIdentifiers(stmt));
}

function bodyOf(code: string): t.Statement[] {
  const ast = parseSync(code, { sourceType: "unambiguous" });
  if (!ast || ast.type !== "File") throw new Error("parse failed");
  const wrapper = findWrapperFunction(ast);
  return wrapper && t.isBlockStatement(wrapper.functionPath.node.body)
    ? wrapper.functionPath.node.body.body
    : ast.program.body;
}

/** hash → its single unanimous prior file, absent when spread/collided. */
function unanimousFileByHash(ledger: Ledger): Map<string, string> {
  const filesByHash = new Map<string, Set<string>>();
  const hashes = ledger.hashes ?? [];
  for (let i = 0; i < hashes.length; i++) {
    let set = filesByHash.get(hashes[i]);
    if (!set) {
      set = new Set();
      filesByHash.set(hashes[i], set);
    }
    set.add(ledger.order[i]);
  }
  const out = new Map<string, string>();
  for (const [hash, files] of filesByHash) {
    if (files.size === 1) out.set(hash, [...files][0]);
  }
  return out;
}

function analyzePair(
  pair: string,
  freshLedger: Ledger,
  freshCode: string,
  priorLedger: Ledger
) {
  const priorHome = new Map(
    Object.entries(priorLedger.nameToFiles).map(([k, v]) => [k, v[0]])
  );
  const freshHome = new Map(
    Object.entries(freshLedger.nameToFiles).map(([k, v]) => [k, v[0]])
  );
  const body = bodyOf(freshCode);
  const stmtIndexByName = new Map<string, number>();
  body.forEach((stmt, i) => {
    for (const name of declaredNamesOf(stmt)) {
      if (!stmtIndexByName.has(name)) stmtIndexByName.set(name, i);
    }
  });
  const priorUnanimous = unanimousFileByHash(priorLedger);
  const freshHashes = freshLedger.hashes ?? [];

  let reloc = 0;
  let strict = 0;
  let either = 0;
  let ownHashUnanimous = 0;
  let noStatement = 0;
  let priorMultiFile = 0;
  let statementNovel = 0;
  const priorHashSet = new Set(priorLedger.hashes ?? []);
  for (const [name, home] of freshHome) {
    const prior = priorHome.get(name);
    if (prior === undefined || prior === home) continue;
    reloc++;
    if ((priorLedger.nameToFiles[name]?.length ?? 0) > 1) priorMultiFile++;
    const idx = stmtIndexByName.get(name);
    if (idx === undefined) {
      noStatement++;
      continue;
    }
    if (!priorHashSet.has(freshHashes[idx])) statementNovel++;
    const own = priorUnanimous.get(freshHashes[idx]);
    if (own === prior) ownHashUnanimous++;
    const prev = idx > 0 ? priorUnanimous.get(freshHashes[idx - 1]) : undefined;
    const next =
      idx + 1 < freshHashes.length
        ? priorUnanimous.get(freshHashes[idx + 1])
        : undefined;
    if (prev === prior && next === prior) strict++;
    if (
      (prev === prior || next === prior) &&
      (prev === undefined || prev === prior || next === prior) &&
      (next === undefined || next === prior || prev === prior)
    ) {
      // at least one neighbor agrees and neither contradicts
      if (
        (prev === prior || prev === undefined) &&
        (next === prior || next === undefined) &&
        (prev === prior || next === prior)
      ) {
        either++;
      }
    }
  }
  return {
    pair,
    reloc,
    strict,
    either,
    ownHashUnanimous,
    noStatement,
    priorMultiFile,
    statementNovel
  };
}

function main() {
  const cfg = JSON.parse(
    fs.readFileSync(path.join(HERE, "pairs.json"), "utf8")
  );
  const freshRoot = process.argv[2]; // e.g. /tmp/eval-work/lever1-twin-v2
  const results = [];
  for (const p of cfg.pairs) {
    const priorLedgerPath = path.join(
      cfg.priorsBase,
      `claude-code-${p.from}/.humanify/split-ledger.json`
    );
    const freshBase =
      freshRoot && fs.existsSync(path.join(freshRoot, p.to))
        ? path.join(freshRoot, p.to, ".humanify")
        : path.join(cfg.priorsBase, `claude-code-${p.to}/.humanify`);
    const freshLedgerPath = path.join(freshBase, "split-ledger.json");
    const freshCodePath = path.join(freshBase, "humanified.js");
    if (!fs.existsSync(priorLedgerPath) || !fs.existsSync(freshLedgerPath)) {
      console.error(`SKIP ${p.from}->${p.to}`);
      continue;
    }
    console.error(`analyzing ${p.from}->${p.to} (${freshBase}) ...`);
    results.push(
      analyzePair(
        `${p.from}->${p.to}`,
        JSON.parse(fs.readFileSync(freshLedgerPath, "utf8")),
        fs.readFileSync(freshCodePath, "utf8"),
        JSON.parse(fs.readFileSync(priorLedgerPath, "utf8"))
      )
    );
  }
  console.log(
    "pair             reloc  strict  either  ownUnanim  multiFile  stmtNovel"
  );
  const tot = { reloc: 0, strict: 0, either: 0, own: 0, multi: 0, novel: 0 };
  for (const r of results) {
    console.log(
      `${r.pair.padEnd(17)}${String(r.reloc).padEnd(7)}${String(r.strict).padEnd(8)}${String(r.either).padEnd(8)}${String(r.ownHashUnanimous).padEnd(11)}${String(r.priorMultiFile).padEnd(11)}${r.statementNovel}`
    );
    tot.reloc += r.reloc;
    tot.strict += r.strict;
    tot.either += r.either;
    tot.own += r.ownHashUnanimous;
    tot.multi += r.priorMultiFile;
    tot.novel += r.statementNovel;
  }
  console.log(
    `TOTAL            ${String(tot.reloc).padEnd(7)}${String(tot.strict).padEnd(8)}${String(tot.either).padEnd(8)}${String(tot.own).padEnd(11)}${String(tot.multi).padEnd(11)}${tot.novel}`
  );
}

main();
