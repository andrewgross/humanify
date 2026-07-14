import assert from "node:assert";
import { describe, it } from "node:test";
import { analyzeEnvReads } from "./analyze.js";
import { formatEnvReadsReport } from "./format.js";

const SOURCE = [
  "const a = process.env.FOO;",
  "log(process.env.FOO);",
  "const b = process.env.BAR;",
  "const c = process.env[dyn];",
  "const keys = Object.keys(process.env);"
].join("\n");

describe("formatEnvReadsReport", () => {
  const report = analyzeEnvReads([{ file: "app.js", code: SOURCE }]);

  it("renders text with variables, both read sites, and the sections", () => {
    const text = formatEnvReadsReport(report);
    assert.match(text, /2 file\(s\)|1 file\(s\), 2 variable\(s\)/);
    assert.match(text, /Variables \(2\)/);
    // FOO is read twice — both locations listed.
    assert.match(text, /FOO\n {4}app\.js:1\n {4}app\.js:2/);
    assert.match(text, /Dynamic keys \(1\) — computed at runtime/);
    assert.match(text, /Whole-env \/ enumerated uses \(1\)/);
  });

  it("renders Markdown headings and code-fenced names", () => {
    const md = formatEnvReadsReport(report, { markdown: true });
    assert.match(md, /^# Environment variable reads/m);
    assert.match(md, /## Variables \(2\)/);
    assert.match(md, /- `FOO` — app\.js:1, app\.js:2/);
    assert.match(md, /## Dynamic keys \(1\)/);
  });

  it("omits empty sections", () => {
    const clean = analyzeEnvReads([{ file: "x.js", code: "const y = 1;" }]);
    const text = formatEnvReadsReport(clean);
    assert.doesNotMatch(text, /Dynamic keys/);
    assert.doesNotMatch(text, /Variables \(/);
  });
});
