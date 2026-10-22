// exp136: the NAME EVIDENCE of bundle units, read by a real JS tokenizer (acorn): string and template-chunk values,
// `.property` names and object-literal keys (`{key:` / `,key:`; a data module's evidence). stdin: JSON {"bundle": path, "spans": [[startByte, endByte], ...]}; stdout: JSON list of
// token lists (one per span; [] when the span does not tokenize). Run with NODE_PATH holding acorn.
import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
const require = createRequire(import.meta.url);
const acorn = require("acorn");
const req = JSON.parse(readFileSync(0, "utf8"));
const buf = readFileSync(req.bundle);
const out = [];
for (const [s, e] of req.spans) {
  const text = buf.subarray(s, e).toString("utf8");
  const toks = new Set();
  {
    const all = [];
    try {
      for (const t of acorn.tokenizer(text, { ecmaVersion: "latest", allowReturnOutsideFunction: true,
        allowHashBang: true, allowAwaitOutsideFunction: true, sourceType: "script" })) all.push(t);
    } catch {
      // a span that does not tokenize keeps what was read before the error
    }
    for (let k = 0; k < all.length; k++) {
      const t = all[k];
      const label = t.type.label;
      const prev = k > 0 ? all[k - 1].type.label : "";
      const next = k + 1 < all.length ? all[k + 1].type.label : "";
      if ((label === "string" || label === "template") && typeof t.value === "string") toks.add(t.value);
      else if (label === "name" && t.value.length >= 4 && (prev === "." || prev === "?." ||
               (next === ":" && (prev === "{" || prev === ",")))) toks.add(t.value);
    }
  }
  out.push([...toks]);
}
process.stdout.write(JSON.stringify(out));
