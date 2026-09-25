// WP5.1 capture hook (preloaded with `tsx --import`): records every
// `stableSplitFromCode` call the TS split's own test suite makes — its
// inputs (code, prior ledger, prior carry, cluster knobs, the applied kill
// switches, the fossil flag), the TS statement hashes of the wrapper body,
// every namer/reviser exchange, the placement trail (spans converted to
// UTF-8 bytes) and the per-statement assignment — as one JSON line each
// in $WP51_CAPTURE_OUT. The Rust placement tests replay these calls: the
// TS spec's scenarios become exact decision vectors.
//
//   WP51_CAPTURE_OUT=test/parity/wp51-split-calls.jsonl WP51_SRC=$PWD \
//     npx tsx --import ./test/parity/wp51-capture-hook.mjs --test \
//     src/split/stable-split.test.ts src/split/split-boots.test.ts \
//     src/split/fossil-split-integration.test.ts
import fs from "node:fs";
import { register } from "node:module";

const out = process.env.WP51_CAPTURE_OUT;
const src = process.env.WP51_SRC;
if (!out || !src) throw new Error("WP51_CAPTURE_OUT / WP51_SRC unset");
fs.writeFileSync(out, "");

register(new URL("./wp51-capture-loader.mjs", import.meta.url));

const t = await import("@babel/types");
const { parseFileAst } = await import(`${src}/src/babel-utils.ts`);
const { findWrapperFunction } = await import(
  `${src}/src/analysis/wrapper-detection.ts`
);
const { statementHash } = await import(`${src}/src/split/statement-hash.ts`);
const { ByteOffsetTable } = await import(`${src}/src/dump/spans.ts`);
const { activeKillSwitches } = await import(`${src}/src/kill-switches.ts`);

globalThis.__wp51rec = (call) => {
  const { code, options, namerLog, result, trail, error } = call;
  const table = ByteOffsetTable.for(code);
  const bytes = (s) =>
    s ? { start: table.toByte(s.start), end: table.toByte(s.end) } : null;
  const ast = parseFileAst(code);
  const wrapper = ast ? findWrapperFunction(ast) : null;
  const body = wrapper?.functionPath.node.body;
  const hashes =
    body && t.isBlockStatement(body) ? body.body.map(statementHash) : null;
  const row = {
    code,
    fossil: options.fossil === true,
    prior: options.prior ?? null,
    priorCarry: options.priorCarry
      ? {
          statementTexts: options.priorCarry.statementTexts,
          matchMap: [...options.priorCarry.matchMap.entries()]
        }
      : null,
    clusterConfig: options.clusterConfig ?? null,
    namer: options.namer !== undefined,
    reviser: options.reviser !== undefined,
    mintNamer: options.mintNamer !== undefined,
    disabled: activeKillSwitches(),
    hashes,
    namerLog: namerLog.map((e) => ({
      kind: e.kind,
      arg: e.arg,
      out: e.out
    })),
    assignment: result ? result.ledger.order : null,
    trail: trail.trails.map((e) => ({ ...e, span: bytes(e.span) })),
    error: error ?? null
  };
  fs.appendFileSync(out, `${JSON.stringify(row)}\n`);
};
