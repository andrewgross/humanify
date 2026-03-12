import assert from "node:assert";
import { describe, it } from "node:test";
import { nameCluster } from "./naming.js";
import type { Cluster } from "./types.js";

function makeCluster(overrides: Partial<Cluster> & { id: string }): Cluster {
  return {
    rootFunctions: [],
    members: new Set(),
    memberHashes: [],
    ...overrides
  };
}

describe("nameCluster", () => {
  it("single root with humanified name → uses that name", () => {
    const cluster = makeCluster({
      id: "abc123def456abc1",
      rootFunctions: ["test.js:2:0"]
    });
    const names = new Map([["test.js:2:0", "createAuth"]]);

    const name = nameCluster(cluster, names);
    assert.strictEqual(name, "createAuth.js");
  });

  it("multiple roots → common prefix", () => {
    const cluster = makeCluster({
      id: "abc123def456abc1",
      rootFunctions: ["test.js:2:0", "test.js:5:0"]
    });
    const names = new Map([
      ["test.js:2:0", "authLogin"],
      ["test.js:5:0", "authLogout"]
    ]);

    const name = nameCluster(cluster, names);
    assert.strictEqual(name, "auth.js");
  });

  it("multiple roots with no common prefix → joined names", () => {
    const cluster = makeCluster({
      id: "abc123def456abc1",
      rootFunctions: ["test.js:2:0", "test.js:5:0"]
    });
    const names = new Map([
      ["test.js:2:0", "createUser"],
      ["test.js:5:0", "deletePost"]
    ]);

    const name = nameCluster(cluster, names);
    assert.strictEqual(name, "createUser_deletePost.js");
  });

  it("fallback → mod_<fingerprint>.js", () => {
    const cluster = makeCluster({
      id: "abc123def456abc1",
      rootFunctions: ["test.js:2:0"]
    });
    // No names available
    const names = new Map<string, string>();

    const name = nameCluster(cluster, names);
    assert.strictEqual(name, "mod_abc123def456abc1.js");
  });

  it("single root with minified-looking name → fallback", () => {
    const cluster = makeCluster({
      id: "abc123def456abc1",
      rootFunctions: ["test.js:2:0"]
    });
    const names = new Map([["test.js:2:0", "a"]]);

    const name = nameCluster(cluster, names);
    assert.strictEqual(name, "mod_abc123def456abc1.js");
  });

  it("common prefix too short → joined names", () => {
    const cluster = makeCluster({
      id: "abc123def456abc1",
      rootFunctions: ["test.js:2:0", "test.js:5:0"]
    });
    const names = new Map([
      ["test.js:2:0", "getUserData"],
      ["test.js:5:0", "getPostList"]
    ]);

    const name = nameCluster(cluster, names);
    // "get" is the common prefix, but it's too generic (3 chars)
    // Should fall back to joined
    assert.strictEqual(name, "getUserData_getPostList.js");
  });
});
