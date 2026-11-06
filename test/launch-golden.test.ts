import assert from "node:assert";
import { spawnSync } from "node:child_process";
import { describe, it } from "node:test";

import { shellQuote } from "../scripts/lib/launch-golden.js";

describe("shellQuote (the e2e stage's launch log)", () => {
  it("writes each argument exactly as bash's printf %q does", () => {
    const samples = [
      "",
      "plain",
      "/a/b-c_d.e/index.js",
      "--prior-version",
      "http://127.0.0.1:8000/v1",
      "a=b",
      "a,b",
      "~x",
      "x~",
      "#a",
      "a#",
      "a b",
      "it's",
      'q"q',
      "a:b@c%d+e",
      "a^b",
      "x{y}",
      "$HOME",
      "a;b|c&d",
      "(x)<y>!*?[z]`w`\\",
      "é"
    ];
    const bash = spawnSync(
      "bash",
      ["-c", 'for a in "$@"; do printf "%q\\n" "$a"; done', "_", ...samples],
      { encoding: "utf8", env: { ...process.env, LC_ALL: "C.UTF-8" } }
    );
    assert.strictEqual(bash.status, 0, bash.stderr);
    assert.deepStrictEqual(
      samples.map(shellQuote),
      bash.stdout.split("\n").slice(0, samples.length)
    );
  });

  it("refuses a control character instead of guessing bash's $'...' form", () => {
    assert.throws(() => shellQuote("a\nb"), /control character/);
  });
});
