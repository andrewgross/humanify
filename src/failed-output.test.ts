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
