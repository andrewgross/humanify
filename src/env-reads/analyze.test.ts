import assert from "node:assert";
import { describe, it } from "node:test";
import { analyzeEnvReads, type EnvReadsReport } from "./analyze.js";

/** Analyze a single in-memory source and return the report. */
function analyze(code: string): EnvReadsReport {
  return analyzeEnvReads([{ file: "in.js", code }]);
}

/** The variable names the report resolved, sorted. */
function names(report: EnvReadsReport): string[] {
  return report.byVar.map((v) => v.name);
}

describe("analyzeEnvReads — member access", () => {
  it("resolves direct member reads (process.env.FOO)", () => {
    const r = analyze("const a = process.env.FOO;\nconst b = process.env.BAR;");
    assert.deepStrictEqual(names(r), ["BAR", "FOO"]);
  });

  it("resolves computed string-literal keys (process.env['BAZ'])", () => {
    const r = analyze('const a = process.env["BAZ"];');
    assert.deepStrictEqual(names(r), ["BAZ"]);
  });

  it("flags computed dynamic keys as unresolvable, not as a var", () => {
    const r = analyze("function f(k) { return process.env[k]; }");
    assert.deepStrictEqual(names(r), []);
    assert.strictEqual(r.dynamic.length, 1);
    assert.match(r.dynamic[0].snippet, /process\.env\[k\]/);
  });

  it("aggregates every read site of the same var", () => {
    const r = analyze(
      "process.env.TOKEN;\nif (process.env.TOKEN) {}\nlog(process.env.TOKEN);"
    );
    assert.deepStrictEqual(names(r), ["TOKEN"]);
    assert.strictEqual(r.byVar[0].locations.length, 3);
    assert.deepStrictEqual(
      r.byVar[0].locations.map((l) => l.line),
      [1, 2, 3]
    );
  });
});

describe("analyzeEnvReads — destructuring", () => {
  it("resolves destructured keys", () => {
    const r = analyze("const { HOME, PATH } = process.env;");
    assert.deepStrictEqual(names(r), ["HOME", "PATH"]);
  });

  it("treats a rest element as an enumerated (whole-env) use", () => {
    const r = analyze("const { HOME, ...rest } = process.env;");
    assert.deepStrictEqual(names(r), ["HOME"]);
    assert.strictEqual(r.enumerated.length, 1);
  });
});

describe("analyzeEnvReads — alias resolution", () => {
  it("follows an aliased env binding to its member reads and destructures", () => {
    const r = analyze(
      "const e = process.env;\nconst x = e.A;\nconst { B } = e;\ne['C'];"
    );
    assert.deepStrictEqual(names(r), ["A", "B", "C"]);
  });

  it("follows a chain of aliases", () => {
    const r = analyze("const e = process.env;\nconst e2 = e;\ne2.DEEP;");
    assert.deepStrictEqual(names(r), ["DEEP"]);
  });
});

describe("analyzeEnvReads — bases and enumeration", () => {
  it("recognizes Bun.env and import.meta.env", () => {
    const r = analyze("Bun.env.RUNTIME;\nimport.meta.env.MODE;");
    assert.deepStrictEqual(names(r), ["MODE", "RUNTIME"]);
  });

  it("reports whole-env / enumerated uses", () => {
    const r = analyze("const keys = Object.keys(process.env);");
    assert.deepStrictEqual(names(r), []);
    assert.strictEqual(r.enumerated.length, 1);
  });

  it("ignores a locally shadowed process/Bun (not the global env)", () => {
    const r = analyze(
      "function f() { const process = { env: {} }; return process.env.NOPE; }"
    );
    assert.deepStrictEqual(names(r), []);
  });

  it("counts the number of files analyzed", () => {
    const r = analyzeEnvReads([
      { file: "a.js", code: "process.env.A;" },
      { file: "b.js", code: "process.env.B;" }
    ]);
    assert.strictEqual(r.filesAnalyzed, 2);
    assert.deepStrictEqual(names(r), ["A", "B"]);
    assert.deepStrictEqual(
      r.byVar.map((v) => v.locations[0].file),
      ["a.js", "b.js"]
    );
  });
});
