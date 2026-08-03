import assert from "node:assert";
import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import { after, describe, it } from "node:test";
import { preserveFailedOutput } from "./failed-output.js";

/**
 * A file that violates the rename invariant must SURVIVE for inspection.
 *
 * The pipeline prints "output was written for inspection, but this run is
 * marked failed" — and then the split consumes and DELETES the offending file.
 * That claim was false, and it cost a real investigation: the 2.1.198 capture
 * was diagnosed down to a token position with both contexts, and the code it
 * referred to no longer existed anywhere on disk.
 *
 * Diagnosing a capture needs BOTH sides — the pre-rename input and the
 * post-rename output — because the failure IS that two bindings which differed
 * in one became identical in the other. One side alone shows nothing.
 */
const tmp = fs.mkdtempSync(path.join(os.tmpdir(), "failedout-"));
after(() => fs.rmSync(tmp, { recursive: true, force: true }));

function tree(name: string, files: Record<string, string>): string {
  const root = path.join(tmp, name);
  for (const [rel, content] of Object.entries(files)) {
    const p = path.join(root, rel);
    fs.mkdirSync(path.dirname(p), { recursive: true });
    fs.writeFileSync(p, content);
  }
  return root;
}

describe("preserveFailedOutput", () => {
  it("keeps both the renamed output and the pre-rename original", () => {
    const out = tree("preserve", {
      "runtime.js": "var b = 1;\nvar q = b !== b;\n"
    });
    preserveFailedOutput(out, [
      {
        filePath: path.join(out, "runtime.js"),
        originalCode: "var a = 1;\nvar q = a !== b;\n"
      }
    ]);
    const dir = path.join(out, ".humanify", "failed");
    assert.ok(
      fs.existsSync(path.join(dir, "runtime.js")),
      "the renamed output must be kept"
    );
    assert.match(
      fs.readFileSync(path.join(dir, "runtime.js.original"), "utf8"),
      /a !== b/,
      "the PRE-rename source must be kept too — a capture cannot be read from one side"
    );
  });

  it("keeps the CODE THE CHECK SAW, which is not the file on disk", () => {
    // Measured on the exp059 capture: the checked code had 16,384,801 tokens
    // and the written file had 16,120,630, because reconcile / the deferred
    // sweep / the family permutation all replace the output AFTER validation.
    // Diffing the file against the original reported a divergence at token 145
    // — a variable-declaration merge unrelated to the failure — while the real
    // one was at 308,757. `.validated` is the artifact that pairs with
    // `.original`; the file is kept only for reference.
    const out = tree("validated", {
      "runtime.js": "var LATER_PASSES_CHANGED_ME = 1;\n"
    });
    preserveFailedOutput(out, [
      {
        filePath: path.join(out, "runtime.js"),
        originalCode: "var a = 1;\nvar q = a !== b;\n",
        validatedCode: "var b = 1;\nvar q = b !== b;\n"
      }
    ]);
    const dir = path.join(out, ".humanify", "failed");
    assert.match(
      fs.readFileSync(path.join(dir, "runtime.js.validated"), "utf8"),
      /b !== b/,
      "the checked code must be preserved verbatim"
    );
    assert.ok(
      !fs
        .readFileSync(path.join(dir, "runtime.js.validated"), "utf8")
        .includes("LATER_PASSES_CHANGED_ME"),
      "the validated copy must NOT be the on-disk file"
    );
  });

  it("never throws when the file is already gone", () => {
    // Best-effort by construction: preservation must not turn a reportable
    // failure into a crash and lose the original error.
    const out = tree("missing", { "keep.js": "1;\n" });
    assert.doesNotThrow(() =>
      preserveFailedOutput(out, [
        { filePath: path.join(out, "gone.js"), originalCode: "var a = 1;\n" }
      ])
    );
  });

  it("does nothing when there are no failures", () => {
    const out = tree("clean", { "a.js": "1;\n" });
    preserveFailedOutput(out, []);
    assert.ok(!fs.existsSync(path.join(out, ".humanify", "failed")));
  });
});
