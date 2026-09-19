import assert from "node:assert";
import { createHash } from "node:crypto";
import { describe, it } from "node:test";
import { DUMP_SCHEMA_VERSION, spanKeyOrder } from "./serialize.js";
import { sha256Hex } from "../rename/rename-ledger.js";

describe("spanKeyOrder", () => {
  it("orders by (text, start, end) — never insertion order", () => {
    const fresh = (start: number, end: number) => ({
      text: "fresh",
      start,
      end
    });
    const prior = (start: number, end: number) => ({
      text: "prior",
      start,
      end
    });
    const keys = [fresh(5, 9), prior(0, 3), fresh(0, 3), fresh(0, 9)];
    const sorted = [...keys].sort(spanKeyOrder);
    assert.deepStrictEqual(
      sorted.map((k) => [k.text, k.start, k.end]),
      [
        ["fresh", 0, 3],
        ["fresh", 0, 9],
        ["fresh", 5, 9],
        ["prior", 0, 3]
      ]
    );
  });

  it("is a total order: equal keys compare 0 both ways", () => {
    const a = { text: "fresh", start: 3, end: 9 };
    const b = { text: "fresh", start: 3, end: 9 };
    assert.strictEqual(spanKeyOrder(a, b), 0);
    assert.strictEqual(spanKeyOrder(b, a), 0);
  });
});

describe("sha256Hex", () => {
  it("matches Node's createHash sha256 hex digest", () => {
    const text = "const x = 1;";
    assert.strictEqual(
      sha256Hex(text),
      createHash("sha256").update(text).digest("hex")
    );
  });
});

describe("DUMP_SCHEMA_VERSION", () => {
  it("is a number, bumped deliberately (12 §2 tableVersion discipline)", () => {
    assert.strictEqual(typeof DUMP_SCHEMA_VERSION, "number");
    assert.ok(DUMP_SCHEMA_VERSION >= 1);
  });
});
