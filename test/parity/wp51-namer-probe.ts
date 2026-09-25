// WP5.2 namer probe: the TS split namer's exact provider requests (prompt
// bytes, identifiers, usedNames in Set order) and their cache keys, plus
// how it maps answers back, for a set of request batches — and the same
// for the tree reviser. Output: test/parity/wp51-namer.json.
//
//   npx tsx test/parity/wp51-namer-probe.ts > test/parity/wp51-namer.json
import { cacheKeyOf } from "../../src/llm/cached-provider.js";
import type { BatchRenameRequest, LLMProvider } from "../../src/llm/types.js";
import {
  createSplitNamer,
  createTreeReviser
} from "../../src/split/split-namer.js";
import type { SplitNameRequest } from "../../src/split/stable-split.js";

const PARAMS = {
  model: "openai/gpt-oss-20b",
  temperature: 0,
  reasoningEffort: "low"
};

const FILE: SplitNameRequest = {
  kind: "file",
  mechanicalStem: "handleMessageVal",
  siblings: ["createTeammateTag", "index"],
  bindings: ["function handleMessage (12 refs)", "var messageQueue (3 refs)"]
};
const FOLDER: SplitNameRequest = {
  kind: "folder",
  mechanicalStem: "rgbString",
  siblings: ["persistToDisk", "index"],
  bindings: ["function rgbString (8 refs)", "function hslToRgb (5 refs)"],
  members: ["rgbString", "hslToRgb", "parseColor"]
};

const batches: SplitNameRequest[][] = [
  [FILE],
  [FILE, FOLDER],
  [
    FILE,
    { ...FILE, siblings: [] },
    { ...FILE, mechanicalStem: "HandleMessageVal" }
  ],
  [
    { ...FOLDER, level: "top" },
    { ...FOLDER, level: "sub", members: [] }
  ],
  [
    {
      kind: "file",
      mechanicalStem: "handlerVal",
      siblings: [],
      bindings: [],
      evidence: 'strings: "exponential jitter retry"; calls: Math.floor'
    },
    {
      kind: "file",
      mechanicalStem: "empty-ev",
      siblings: ["a", "b", "a"],
      bindings: ["x"],
      evidence: ""
    }
  ]
];

// Answers: echo, stem, a real name, empty, missing — per key position.
function answers(req: BatchRenameRequest): Record<string, string> {
  const out: Record<string, string> = {};
  req.identifiers.forEach((k, i) => {
    if (i % 4 === 0) out[k] = `proposal${i}Name`;
    else if (i % 4 === 1) out[k] = k;
    else if (i % 4 === 2) out[k] = "";
  });
  return out;
}

const rows: unknown[] = [];
for (const batch of batches) {
  let seen: BatchRenameRequest | undefined;
  const provider: LLMProvider = {
    async suggestAllNames(req) {
      seen = req;
      return { renames: answers(req) };
    }
  };
  const result = await createSplitNamer(provider)(batch);
  if (!seen) throw new Error("no request");
  rows.push({
    kind: "namer",
    requests: batch,
    request: {
      code: seen.code,
      identifiers: seen.identifiers,
      usedNames: [...seen.usedNames],
      systemPrompt: seen.systemPrompt,
      userPrompt: seen.userPrompt
    },
    cacheKey: cacheKeyOf(seen, PARAMS),
    answers: answers(seen),
    result
  });
}

const folders = [
  { name: "auth", members: ["login", "logout", "session"] },
  { name: "conn", members: ["socket", "pool"] },
  { name: "same", members: [] }
];
{
  let seen: BatchRenameRequest | undefined;
  const provider: LLMProvider = {
    async suggestAllNames(req) {
      seen = req;
      return { renames: { auth: "authFlow", conn: "conn", same: "" } };
    }
  };
  const result = await createTreeReviser(provider)(folders);
  if (!seen) throw new Error("no reviser request");
  rows.push({
    kind: "reviser",
    folders,
    request: {
      code: seen.code,
      identifiers: seen.identifiers,
      usedNames: [...seen.usedNames],
      systemPrompt: seen.systemPrompt,
      userPrompt: seen.userPrompt
    },
    cacheKey: cacheKeyOf(seen, PARAMS),
    result
  });
}
process.stdout.write(`${JSON.stringify({ params: PARAMS, rows }, null, 1)}\n`);
