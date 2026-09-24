/**
 * WP4.1 probes: the JS behaviors the Rust LLM layer must reproduce
 * byte-for-byte, recorded from the REAL TS functions (not re-derived):
 *
 *   canonical — src/llm/cached-provider.ts canonicalJson + cacheKeyOf on
 *     adversarial inputs: array-index-like keys (JSON.stringify enumerates
 *     them FIRST, numerically, whatever the sort put first), UTF-16 vs
 *     byte key order (U+FF5E vs U+1F600), control characters, U+2028,
 *     temperatures 0 / -0 / 0.7 / 1e-7 / 1e21 / 0.000001, Set members.
 *   stringify — JSON.stringify of a parsed cache entry (the TS write path)
 *     for index-like keys and duplicates.
 *   number — String(x) for the JS number formatter.
 *   toFixed / formatDuration / formatTokens — metrics.ts's formatting
 *     (toFixed rounds the EXACT binary value half-up: 1150/1000 → "1.1",
 *     1250/1000 → "1.3").
 *   entries — the OpenAI adapter's Object.entries(JSON.parse(content)) on
 *     non-object JSON (a bare string yields one entry PER CHARACTER).
 *
 *   npx tsx test/parity/wp41-js-probe.ts > test/parity/wp41-js-vectors.json
 */
import {
  canonicalJson,
  cacheKeyOf,
  type CacheKeyParams
} from "../../src/llm/cached-provider.js";
import { formatDuration, formatTokens } from "../../src/llm/metrics.js";
import type { BatchRenameRequest } from "../../src/llm/types.js";

const base: BatchRenameRequest = {
  code: "function f() {}",
  identifiers: ["f"],
  usedNames: new Set(["console"]),
  calleeSignatures: [],
  callsites: []
};

const canonicalCases: Array<[string, unknown]> = [
  ["index-keys-first", { b: 1, "10": 2, "9": 3, a: 4, "01": 5, "-1": 6 }],
  ["index-key-bounds", { "4294967294": 1, "4294967295": 2, "0": 3, z: 4 }],
  ["utf16-order", { "\u{1F600}": 1, "～": 2, é: 3, z: 4, Z: 5 }],
  ["controls", { s: 'a\u0000\u0001\u001f\u007f\b\f\n\r\t"\\/  é😀' }],
  ["nested", { z: [{ b: 1, a: [3, 1, 2] }, "x"], a: null, m: true }],
  ["empty", { a: [], b: {} }],
  [
    "numbers",
    {
      a: 0,
      b: -0,
      c: 0.7,
      d: 1e-7,
      e: 1e21,
      f: 0.000001,
      g: 123456789012345680000,
      h: 1.5e300,
      i: 2 ** 53 + 2,
      j: -3.25
    }
  ]
];

const keyCases: Array<[string, CacheKeyParams, BatchRenameRequest]> = [
  ["temperature-0", { model: "m", temperature: 0 }, base],
  ["temperature-neg0", { model: "m", temperature: -0 }, base],
  ["temperature-0.7", { model: "m", temperature: 0.7 }, base],
  ["temperature-1e-7", { model: "m", temperature: 1e-7 }, base],
  ["temperature-absent", { model: "m" }, base],
  [
    "callee-snippet",
    { model: "m", temperature: 0 },
    {
      ...base,
      calleeSignatures: [
        {
          name: "g",
          params: ["a"],
          snippet: "{\n  return a;\n}"
        } as { name: string; params: string[] }
      ]
    }
  ],
  [
    "used-names-utf16-sort",
    { model: "m", temperature: 0 },
    { ...base, usedNames: new Set(["\u{1F600}", "～", "b", "B", "_", "$"]) }
  ],
  [
    "maps-index-keys",
    { model: "m", temperature: 0 },
    { ...base, alreadyRenamed: { b: "x", a: "y" }, priorNameHints: { z: "q" } }
  ]
];

const numbers = [
  0,
  -0,
  1,
  -1,
  0.1,
  0.7,
  1e-7,
  1e-6,
  1.5e-7,
  123e-20,
  1e21,
  1e20,
  123456789,
  2 ** 53,
  1 / 3,
  5e-324,
  1.7976931348623157e308,
  100,
  1e15,
  12345.678
];
const fixed: Array<[number, number]> = [
  [1.15, 1],
  [1.25, 1],
  [1.05, 1],
  [0.05, 1],
  [2.5, 0],
  [999.95, 1],
  [1234.5678, 1]
];
const durations = [
  0, 500, 999, 1000, 1050, 1150, 1250, 5000, 59999, 60000, 125000, 3599999,
  3600000, 7500000, 12.5
];
const tokens = [
  0, 999, 1000, 1050, 1150, 1250, 999999, 1000000, 1050000, 1250000
];
const entries = [
  '{"a":"x","b":1,"c":null,"d":"y"}',
  '"ab"',
  "123",
  "null",
  '["p","q"]',
  '{"2":"two","1":"one","z":"zz","1":"uno"}',
  "true"
];

function entriesOf(content: string): unknown {
  try {
    const result = JSON.parse(content);
    const renames: Record<string, string> = {};
    for (const [k, v] of Object.entries(result)) {
      if (typeof v === "string") renames[k] = v;
    }
    return { ok: JSON.stringify(renames) };
  } catch (err) {
    return { threw: (err as Error).constructor.name };
  }
}

const out = {
  canonical: canonicalCases.map(([name, value]) => ({
    name,
    input: JSON.stringify(value),
    canonical: canonicalJson(value)
  })),
  keys: keyCases.map(([name, params, request]) => ({
    name,
    params: JSON.stringify(params),
    tempIsNegZero: Object.is(params.temperature, -0),
    request: JSON.stringify({ ...request, usedNames: [...request.usedNames] }),
    material: canonicalJson({
      cacheVersion: 1,
      params,
      request: { ...request }
    }),
    key: cacheKeyOf(request, params)
  })),
  stringify: [
    '{"v":1,"renames":{"b":"x","10":"t","2":"s","a":"y"},"finishReason":"stop"}',
    '{"v":1,"renames":{"a":"x","b":"y","a":"z"}}'
  ].map((raw) => ({ raw, stringified: JSON.stringify(JSON.parse(raw)) })),
  numbers: numbers.map((n) => ({
    bits: Buffer.from(new Float64Array([n]).buffer).toString("hex"),
    string: String(n)
  })),
  toFixed: fixed.map(([n, d]) => ({
    bits: Buffer.from(new Float64Array([n]).buffer).toString("hex"),
    digits: d,
    fixed: n.toFixed(d)
  })),
  formatDuration: durations.map((ms) => ({ ms, out: formatDuration(ms) })),
  formatTokens: tokens.map((n) => ({ n, out: formatTokens(n) })),
  entries: entries.map((content) => ({
    content,
    ...(entriesOf(content) as object)
  }))
};
console.log(JSON.stringify(out, null, 1));
