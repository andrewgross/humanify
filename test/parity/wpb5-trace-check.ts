// probe: WPB.5's gate — a `--profile` trace is a loadable Chrome
// trace-event file, and the Rust trace has the TS trace's SHAPE.
//
// 1. FORMAT (both files, per the Trace Event Format's JSON Object Format):
//    top level `{ traceEvents: [...] }`; every event has string `name`,
//    string `ph` of a known phase, number `ts` >= 0, integer `pid` and
//    `tid`; `X` carries number `dur` >= 0; `B`/`E` balance per (pid, tid)
//    with matching names; `C` args are all numbers; `M` is a
//    process_name/thread_name with a string `args.name`; `args` (when
//    present) is an object; every tid used by a span has a thread_name.
// 2. SHAPE (Rust vs TS; timings naturally differ): every Rust event's
//    signature — ph + its key list IN ORDER + each value's JSON type — is
//    one the TS trace also contains; the metadata events have the TS
//    layout (process_name first, then one thread_name per used tid, then
//    spans, then counters); every Rust span NAME the TS also emits has the
//    same cat and tid, and args keys a subset of the TS's — a TS key the
//    Rust lacks FAILS unless declared with --allow-missing <span>.<key>.
//
// Usage: npx tsx test/parity/wpb5-trace-check.ts <rust-trace> <ts-trace>
//          [--allow-missing detection.adapter ...]

import { readFileSync } from "node:fs";

type Ev = Record<string, unknown>;
const PHASES = new Set("BEXIiCbneMsftPONDRc".split(""));

function isInt(x: unknown): boolean {
  return typeof x === "number" && Number.isInteger(x);
}

const argsOf = (e: Ev): Ev => (e.args ?? {}) as Ev;

/** Each rule returns true when the event VIOLATES it. */
const EVENT_RULES: [string, (e: Ev) => boolean][] = [
  ["name not a string", (e) => typeof e.name !== "string"],
  ["unknown ph", (e) => typeof e.ph !== "string" || !PHASES.has(e.ph)],
  ["bad ts", (e) => typeof e.ts !== "number" || e.ts < 0],
  ["pid/tid not ints", (e) => !isInt(e.pid) || !isInt(e.tid)],
  ["cat not a string", (e) => e.cat !== undefined && typeof e.cat !== "string"],
  [
    "args not an object",
    (e) =>
      e.args !== undefined && (typeof e.args !== "object" || e.args === null)
  ],
  [
    "X without dur >= 0",
    (e) => e.ph === "X" && (typeof e.dur !== "number" || e.dur < 0)
  ],
  [
    "C args not all numbers",
    (e) =>
      e.ph === "C" &&
      !Object.values(argsOf(e)).every((v) => typeof v === "number")
  ],
  [
    "unexpected metadata name",
    (e) => e.ph === "M" && e.name !== "process_name" && e.name !== "thread_name"
  ],
  [
    "M without string args.name",
    (e) => e.ph === "M" && typeof argsOf(e).name !== "string"
  ]
];

function checkEvent(e: Ev, i: number, errors: string[]): void {
  for (const [message, violates] of EVENT_RULES) {
    if (violates(e)) errors.push(`event ${i} (${String(e.name)}): ${message}`);
  }
}

function checkBalance(events: Ev[], errors: string[]): void {
  const stacks = new Map<string, string[]>();
  for (const e of events) {
    const key = `${e.pid}/${e.tid}`;
    const stack = stacks.get(key) ?? [];
    stacks.set(key, stack);
    if (e.ph === "B") stack.push(String(e.name));
    if (e.ph === "E" && stack.pop() !== e.name)
      errors.push(`unbalanced E ${String(e.name)} on ${key}`);
  }
  for (const [key, s] of stacks)
    if (s.length > 0) errors.push(`unclosed B on ${key}: ${s.join(",")}`);
}

function validate(label: string, file: string): Ev[] {
  const doc = JSON.parse(readFileSync(file, "utf-8")) as {
    traceEvents?: unknown;
  };
  const errors: string[] = [];
  if (!Array.isArray(doc.traceEvents)) {
    console.log(`${label}: FORMAT FAIL — no traceEvents array`);
    process.exit(1);
  }
  const events = doc.traceEvents as Ev[];
  events.forEach((e, i) => {
    checkEvent(e, i, errors);
  });
  checkBalance(events, errors);
  const named = new Set(
    events.filter((e) => e.name === "thread_name").map((e) => e.tid)
  );
  for (const e of events)
    if ((e.ph === "X" || e.ph === "B") && !named.has(e.tid))
      errors.push(`span ${String(e.name)} on unnamed tid ${String(e.tid)}`);
  const phases = [...new Set(events.map((e) => e.ph))].join(",");
  console.log(
    `${label}: ${events.length} events (ph ${phases}) — FORMAT ${errors.length === 0 ? "OK" : "FAIL"}`
  );
  for (const err of errors) console.log(`  ${err}`);
  if (errors.length > 0) process.exit(1);
  return events;
}

function jsonType(v: unknown): string {
  if (v === null) return "null";
  if (Array.isArray(v)) return "array";
  if (typeof v === "number")
    return Number.isInteger(v) ? "int|number" : "number";
  return typeof v;
}

/** ph + ordered keys + value types; ints and floats both read as numbers. */
function signature(e: Ev): string {
  return `${String(e.ph)}{${Object.keys(e)
    .map(
      (k) =>
        `${k}:${k === "args" ? "object" : jsonType(e[k]).replace("int|", "")}`
    )
    .join(",")}}`;
}

/** The layout: M-process, M-threads, X..., C... as a run-length phase string. */
function layout(events: Ev[]): string {
  const tags = events.map((e) =>
    e.ph === "M" ? (e.name === "process_name" ? "P" : "T") : String(e.ph)
  );
  return tags.filter((t, i) => i === 0 || t !== tags[i - 1]).join("");
}

function compareSignatures(rust: Ev[], ts: Ev[], errors: string[]): void {
  const tsSigs = new Set(ts.map(signature));
  for (const sig of new Set(rust.map(signature))) {
    if (!tsSigs.has(sig)) errors.push(`signature not in the TS trace: ${sig}`);
  }
  const tsLayout = layout(ts);
  const rustLayout = layout(rust);
  if (!/^PT+X*C*$/.test(rustLayout) || !/^PT+X*C*$/.test(tsLayout)) {
    errors.push(`layout rust ${rustLayout} / ts ${tsLayout} is not P T+ X* C*`);
  }
  console.log(
    `shape: ${new Set(rust.map(signature)).size} Rust signature(s) vs ${tsSigs.size} TS; layout rust ${rustLayout} ts ${tsLayout}`
  );
}

/** One span both traces emit: same cat/tid, args keys and types agree. */
function compareSpan(
  r: Ev,
  t: Ev,
  allowMissing: Set<string>,
  errors: string[]
): void {
  const name = String(r.name);
  if (r.cat !== t.cat || r.tid !== t.tid) {
    errors.push(
      `span ${name}: cat/tid ${String(r.cat)}/${String(r.tid)} vs TS ${String(t.cat)}/${String(t.tid)}`
    );
  }
  const rArgs = argsOf(r);
  const tArgs = argsOf(t);
  for (const k of Object.keys(rArgs)) {
    if (!(k in tArgs)) errors.push(`span ${name}: args key ${k} not in TS`);
    else if (jsonType(rArgs[k]) !== jsonType(tArgs[k])) {
      errors.push(`span ${name}: args.${k} type differs`);
    }
  }
  for (const k of Object.keys(tArgs).filter((k) => !(k in rArgs))) {
    if (allowMissing.has(`${name}.${k}`)) {
      console.log(`  declared missing: ${name}.${k}`);
    } else errors.push(`span ${name}: TS args key ${k} missing`);
  }
}

function compareShape(
  rust: Ev[],
  ts: Ev[],
  allowMissing: Set<string>
): string[] {
  const errors: string[] = [];
  compareSignatures(rust, ts, errors);
  const tsSpans = new Map(
    ts.filter((e) => e.ph === "X").map((e) => [e.name, e])
  );
  const common = rust.filter((e) => e.ph === "X" && tsSpans.has(e.name));
  for (const r of common) {
    compareSpan(r, tsSpans.get(r.name) as Ev, allowMissing, errors);
  }
  console.log(`  ${common.length} span name(s) in common`);
  return errors;
}

function main(): void {
  const args = process.argv.slice(2);
  const allow = new Set<string>();
  const files: string[] = [];
  for (let i = 0; i < args.length; i++) {
    if (args[i] === "--allow-missing") allow.add(args[++i]);
    else files.push(args[i]);
  }
  if (files.length !== 2) {
    console.error(
      "usage: wpb5-trace-check.ts <rust-trace> <ts-trace> [--allow-missing span.key]"
    );
    process.exit(2);
  }
  const rust = validate("rust", files[0]);
  const ts = validate("ts", files[1]);
  const errors = compareShape(rust, ts, allow);
  for (const e of errors) console.log(`  SHAPE: ${e}`);
  console.log(errors.length === 0 ? "SHAPE OK" : "SHAPE FAIL");
  process.exit(errors.length === 0 ? 0 : 1);
}

main();
