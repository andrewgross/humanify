// exp136: the NAME EVIDENCE of bundle units, read by a real JS tokenizer (acorn): string and template-chunk values
// and `.property` names. stdin: JSON {"bundle": path, "spans": [[startByte, endByte], ...]}; stdout: JSON list of
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
  try {
    let prevDot = false;
    for (const t of acorn.tokenizer(text, { ecmaVersion: "latest", allowReturnOutsideFunction: true,
      allowHashBang: true, allowAwaitOutsideFunction: true, sourceType: "script" })) {
      const label = t.type.label;
      if ((label === "string" || label === "template") && typeof t.value === "string") toks.add(t.value);
      else if (prevDot && label === "name" && t.value.length >= 4) toks.add(t.value);
      prevDot = label === "." || label === "?.";
    }
  } catch {
    // a span that does not tokenize keeps what was read before the error
  }
  out.push([...toks]);
}
process.stdout.write(JSON.stringify(out));
