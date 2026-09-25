// WP5.1/5.2 placement probe: run the TS split's placement
// (`stableSplitFromCode`) on a shipped text against a prior split ledger,
// in any of the three regimes, with the placement trail armed — and write
// a dump-shaped directory the Rust `humanify placement` verb can run on
// and `humanify-parity compare --sections placement` can diff:
//
//   <out>/meta.json          the pair's meta (copied from --meta)
//   <out>/text/shipped.js    the input text
//   <out>/placement.json     the trail, spans converted to UTF-8 bytes
//   <out>/partitions.json    the statementHash family (the hash seam)
//   <out>/prompts.jsonl      every namer/reviser request (null answers)
//   <out>/fossil-modules.json the ledger's fossilModules (tokens included)
//   <out>/stats.json         the split's stats
//
// The namer answers NULL for every request — the replay of a batch the
// cache does not hold — so the probe exercises the assignment exactly as
// a cold-cache warm replay does; each request's bytes and cache key are
// recorded so the Rust namer's can be compared.
//
//   npx tsx test/parity/wp51-placement-probe.ts --shipped <shipped.js> \
//     --out <dir> --meta <meta.json> [--prior-ledger <split-ledger.json>] \
//     [--regime fossil|tiers|cluster] [--prior-text <prior.js>] \
//     [--match-map <prior-match-map.json>]
import fs from "node:fs";
import path from "node:path";
import * as t from "@babel/types";
import { findWrapperFunction } from "../../src/analysis/wrapper-detection.js";
import { parseFileAst } from "../../src/babel-utils.js";
import { ByteOffsetTable } from "../../src/dump/spans.js";
import { cacheKeyOf } from "../../src/llm/cached-provider.js";
import type { BatchRenameRequest, LLMProvider } from "../../src/llm/types.js";
import { placementTrail } from "../../src/split/placement-trail.js";
import {
  createSplitNamer,
  createTreeReviser
} from "../../src/split/split-namer.js";
import {
  type StableSplitLedger,
  stableSplitFromCode
} from "../../src/split/stable-split.js";
import { statementHash } from "../../src/split/statement-hash.js";

const PARAMS = {
  model: "openai/gpt-oss-20b",
  temperature: 0,
  reasoningEffort: "low"
};

function arg(name: string): string | undefined {
  const i = process.argv.indexOf(`--${name}`);
  return i >= 0 ? process.argv[i + 1] : undefined;
}
const shippedPath = arg("shipped");
const out = arg("out");
const metaPath = arg("meta");
if (!shippedPath || !out || !metaPath) throw new Error("usage: see header");
const regime = arg("regime") ?? "fossil";
const code = fs.readFileSync(shippedPath, "utf-8");
const table = ByteOffsetTable.for(code);
const bytes = (s: { start: number; end: number }) => ({
  text: "shipped",
  start: table.toByte(s.start),
  end: table.toByte(s.end)
});

const priorPath = arg("prior-ledger");
const prior: StableSplitLedger | undefined = priorPath
  ? JSON.parse(fs.readFileSync(priorPath, "utf-8"))
  : undefined;

// The prior carry (tiers regime): the prior text's wrapper-body statement
// texts (prior-version.ts `topLevelStatements`) + the rename matcher's map.
function priorStatementTexts(priorText: string): string[] {
  const ast = parseFileAst(priorText);
  const wrapper = ast ? findWrapperFunction(ast) : null;
  const body = wrapper?.functionPath.node.body;
  if (!body || !t.isBlockStatement(body)) return [];
  return body.body.map((s) =>
    s.start != null && s.end != null ? priorText.slice(s.start, s.end) : ""
  );
}
const priorTextPath = arg("prior-text");
const matchMapPath = arg("match-map");
const priorCarry =
  regime === "tiers" && priorTextPath
    ? {
        statementTexts: priorStatementTexts(
          fs.readFileSync(priorTextPath, "utf-8")
        ),
        matchMap: new Map<string, string>(
          matchMapPath
            ? Object.entries(JSON.parse(fs.readFileSync(matchMapPath, "utf-8")))
            : []
        )
      }
    : undefined;

// A provider that records every request and answers nothing.
const prompts: string[] = [];
let seq = 0;
function recorder(functionId: string): LLMProvider {
  return {
    async suggestAllNames(req: BatchRenameRequest) {
      prompts.push(
        JSON.stringify({
          seq: seq++,
          functionId,
          site: "folders",
          round: 1,
          isRetry: false,
          cacheKey: cacheKeyOf(req, PARAMS),
          systemPrompt: req.systemPrompt,
          userPrompt: req.userPrompt,
          identifiers: req.identifiers,
          targets: []
        })
      );
      throw new Error("probe: no cached answer");
    }
  };
}

placementTrail.reset(true);
const result = await stableSplitFromCode(code, {
  fossil: regime === "fossil",
  prior: regime === "cluster" ? undefined : prior,
  namer:
    regime === "cluster"
      ? createSplitNamer(recorder("split-namer"))
      : undefined,
  reviser:
    regime === "cluster"
      ? createTreeReviser(recorder("tree-reviser"))
      : undefined,
  mintNamer:
    regime === "fossil" && prior
      ? createSplitNamer(recorder("split-namer"))
      : undefined,
  priorCarry
});
if (!result) throw new Error("not stable-splittable");

// The statementHash family, bundle order (the TS hash bytes).
const ast = parseFileAst(code);
const wrapper = ast ? findWrapperFunction(ast) : null;
const body = wrapper?.functionPath.node.body;
if (!body || !t.isBlockStatement(body)) throw new Error("no wrapper body");

fs.mkdirSync(path.join(out, "text"), { recursive: true });
fs.copyFileSync(metaPath, path.join(out, "meta.json"));
fs.writeFileSync(path.join(out, "text", "shipped.js"), code);
const trail = placementTrail.report();
fs.writeFileSync(
  path.join(out, "placement.json"),
  JSON.stringify({
    schemaVersion: 1,
    placements: trail.trails
      .map((e) => ({
        key: bytes(e.span ?? { start: -1, end: -1 }),
        index: e.index,
        names: e.names,
        nameCount: e.nameCount,
        placedBy: e.placedBy,
        file: e.file,
        priorFile: e.priorFile,
        priorFileFrom: e.priorFileFrom,
        hashMiss: e.hashMiss,
        alternatives: e.alternatives,
        evidence: e.evidence
      }))
      .sort((a, b) => a.key.start - b.key.start || a.key.end - b.key.end)
  })
);
fs.writeFileSync(
  path.join(out, "partitions.json"),
  JSON.stringify({
    schemaVersion: 1,
    families: [
      {
        family: "statementHash",
        members: body.body.map((s) => ({
          member: bytes({ start: s.start ?? -1, end: s.end ?? -1 }),
          hash: statementHash(s)
        }))
      }
    ]
  })
);
fs.writeFileSync(
  path.join(out, "prompts.jsonl"),
  prompts.map((p) => `${p}\n`).join("")
);
fs.writeFileSync(
  path.join(out, "fossil-modules.json"),
  JSON.stringify(result.ledger.fossilModules ?? null)
);
fs.writeFileSync(path.join(out, "stats.json"), JSON.stringify(result.stats));
console.log(
  `probe: ${regime} ${trail.trails.length} rows, ${result.stats.files} files, ` +
    `${prompts.length} namer request(s) -> ${out}`
);
