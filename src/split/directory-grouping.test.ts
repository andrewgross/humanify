import { describe, it } from "node:test";
import * as assert from "node:assert/strict";
import {
  normalizeOutputPaths,
  groupBySemanticPrefix,
  type DirectoryGroupingOptions
} from "./directory-grouping.js";

describe("normalizeOutputPaths", () => {
  it("strips common prefix from detected module paths", () => {
    const moduleIds = new Map([
      ["f1", "src/helpers/util.ts"],
      ["f2", "src/helpers/math.ts"],
      ["f3", "src/components/app.ts"]
    ]);
    const result = normalizeOutputPaths(moduleIds);
    assert.equal(result.get("f1"), "helpers/util.js");
    assert.equal(result.get("f2"), "helpers/math.js");
    assert.equal(result.get("f3"), "components/app.js");
  });

  it("changes .ts/.tsx extensions to .js", () => {
    const moduleIds = new Map([
      ["f1", "src/app.tsx"],
      ["f2", "src/util.ts"]
    ]);
    const result = normalizeOutputPaths(moduleIds);
    assert.equal(result.get("f1"), "app.js");
    assert.equal(result.get("f2"), "util.js");
  });

  it("preserves shared.js and orphans.js at root", () => {
    const moduleIds = new Map([
      ["f1", "src/app.ts"],
      ["f2", "src/util.ts"],
      ["shared1", "shared.js"],
      ["orphan1", "orphans.js"]
    ]);
    const result = normalizeOutputPaths(moduleIds);
    assert.equal(result.get("shared1"), "shared.js");
    assert.equal(result.get("orphan1"), "orphans.js");
  });

  it("enforces max depth by flattening deep paths", () => {
    const moduleIds = new Map([["f1", "src/a/b/c/d/e/deep.ts"]]);
    const opts: DirectoryGroupingOptions = { maxDepth: 3 };
    const result = normalizeOutputPaths(moduleIds, opts);
    const path = result.get("f1") ?? "";
    // Should have at most 3 directory levels
    const depth = path.split("/").length - 1; // -1 for the filename
    assert.ok(depth <= 3, `Path "${path}" exceeds maxDepth 3`);
  });

  it("handles single-file common prefix removal", () => {
    const moduleIds = new Map([["f1", "src/index.ts"]]);
    const result = normalizeOutputPaths(moduleIds);
    assert.equal(result.get("f1"), "index.js");
  });

  it("passes through already-flat .js names", () => {
    const moduleIds = new Map([
      ["f1", "helpers.js"],
      ["f2", "utils.js"]
    ]);
    const result = normalizeOutputPaths(moduleIds);
    assert.equal(result.get("f1"), "helpers.js");
    assert.equal(result.get("f2"), "utils.js");
  });
});

describe("groupBySemanticPrefix", () => {
  it("groups files sharing a camelCase prefix into directories", () => {
    const names = new Map([
      ["f1", "createStore.js"],
      ["f2", "createContext.js"],
      ["f3", "createReducer.js"],
      ["f4", "useSelector.js"],
      ["f5", "useDispatch.js"]
    ]);
    const result = groupBySemanticPrefix(names);

    // Functions with "create" prefix should be grouped
    const f1 = result.get("f1") ?? "";
    const f2 = result.get("f2") ?? "";
    const f3 = result.get("f3") ?? "";
    assert.equal(f1.split("/")[0], f2.split("/")[0]);
    assert.equal(f2.split("/")[0], f3.split("/")[0]);

    // Functions with "use" prefix should be grouped
    const f4 = result.get("f4") ?? "";
    const f5 = result.get("f5") ?? "";
    assert.equal(f4.split("/")[0], f5.split("/")[0]);
  });

  it("leaves singletons ungrouped", () => {
    const names = new Map([
      ["f1", "main.js"],
      ["f2", "init.js"]
    ]);
    const result = groupBySemanticPrefix(names);
    // No common prefix, should stay flat
    assert.equal(result.get("f1"), "main.js");
    assert.equal(result.get("f2"), "init.js");
  });

  it("preserves shared.js and orphans.js", () => {
    const names = new Map([
      ["f1", "shared.js"],
      ["f2", "orphans.js"],
      ["f3", "createFoo.js"],
      ["f4", "createBar.js"]
    ]);
    const result = groupBySemanticPrefix(names);
    assert.equal(result.get("f1"), "shared.js");
    assert.equal(result.get("f2"), "orphans.js");
  });
});
