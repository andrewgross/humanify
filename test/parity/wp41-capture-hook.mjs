// WP4.1 capture hook (preloaded with `tsx --import`): records the FULL
// typed request of every CachedLLMProvider dispatch — every own property,
// Sets as arrays in their actual order — plus the params and the key the
// TS computes. Needed because the oracle dump's cache-keys.jsonl flattens
// calleeSignatures to {name, params} and drops `snippet`, which IS key
// material (canonicalJson recurses into every own property).
import fs from "node:fs";

const out = process.env.WP41_CAPTURE_OUT;
if (!out) throw new Error("WP41_CAPTURE_OUT unset");
const src = process.env.WP41_SRC;
const mod = await import(`${src}/src/llm/cached-provider.ts`);
const proto = mod.CachedLLMProvider.prototype;
const original = proto.suggestAllNames;
let seq = 0;
proto.suggestAllNames = function (request) {
  const key = mod.cacheKeyOf(request, this.params);
  const row = JSON.stringify(
    { seq: seq++, params: this.params, request, cacheKey: key },
    (_k, v) => (v instanceof Set ? [...v] : v instanceof Map ? Object.fromEntries(v) : v)
  );
  fs.appendFileSync(out, `${row}\n`);
  return original.call(this, request);
};
