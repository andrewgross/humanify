import assert from "node:assert";
import { describe, it } from "node:test";
import { identifierRegex } from "./identifier-regex.js";

describe("identifierRegex", () => {
  it("matches a plain identifier at word boundaries only", () => {
    const re = identifierRegex("target");
    assert.ok(re.test("const target = 1;"));
    assert.ok(!re.test("const targetValue = 1;"));
    assert.ok(!re.test("const myTarget = retarget();"));
  });

  it("matches $-names surrounded by non-word characters", () => {
    assert.ok(identifierRegex("$H").test("const usage = $H + 1;"));
    assert.ok(identifierRegex("$").test("map($, done)"));
    assert.ok(identifierRegex("w$").test("return w$;"));
  });

  it("does not match $-names inside longer identifiers", () => {
    assert.ok(!identifierRegex("$H").test("const other = a$H;"));
    assert.ok(!identifierRegex("$H").test("call($H8);"));
    assert.ok(!identifierRegex("$").test("pay($$);"));
  });
});
