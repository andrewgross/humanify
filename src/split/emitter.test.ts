import assert from "node:assert";
import { describe, it } from "node:test";
import { computeRelativeImportPath } from "./emitter.js";

describe("computeRelativeImportPath", () => {
  it("same directory uses ./", () => {
    assert.strictEqual(
      computeRelativeImportPath("app.js", "utils.js"),
      "./utils.js"
    );
  });

  it("sibling directory uses ../", () => {
    assert.strictEqual(
      computeRelativeImportPath("src/app.js", "lib/utils.js"),
      "../lib/utils.js"
    );
  });

  it("child directory uses ./child/", () => {
    assert.strictEqual(
      computeRelativeImportPath("app.js", "helpers/utils.js"),
      "./helpers/utils.js"
    );
  });

  it("parent directory uses ../", () => {
    assert.strictEqual(
      computeRelativeImportPath("helpers/utils.js", "shared.js"),
      "../shared.js"
    );
  });

  it("nested sibling uses correct traversal", () => {
    assert.strictEqual(
      computeRelativeImportPath("src/components/app.js", "src/helpers/util.js"),
      "../helpers/util.js"
    );
  });

  it("same subdirectory uses ./", () => {
    assert.strictEqual(
      computeRelativeImportPath("src/helpers/a.js", "src/helpers/b.js"),
      "./b.js"
    );
  });

  it("dot-folder target still gets the ./ prefix (else Node reads a package name)", () => {
    assert.strictEqual(
      computeRelativeImportPath("index.js", ".humanify/_bundle.js"),
      "./.humanify/_bundle.js"
    );
    assert.strictEqual(
      computeRelativeImportPath("src/a/b.js", ".humanify/_bundle.js"),
      "../../.humanify/_bundle.js"
    );
  });
});
