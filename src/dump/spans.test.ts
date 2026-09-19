import assert from "node:assert";
import { describe, it } from "node:test";
import { ByteOffsetTable } from "./spans.js";

// `const é中文 = "😀";` — code units:
//   c0 o1 n2 s3 t4 ' '5 é6 中7 文8 ' '9 =10 ' '11 "12 😀13,14 "15 ;16
// UTF-8 bytes:
//   "const " = 6 (0-5), é = 2 (6-7), 中 = 3 (8-10), 文 = 3 (11-13),
//   ' ' = 1 (14), '=' = 1 (15), ' ' = 1 (16), '"' = 1 (17), 😀 = 4 (18-21),
//   '"' = 1 (22), ';' = 1 (23). Total 24 bytes.

describe("ByteOffsetTable (ASCII fast path)", () => {
  it("is the identity mapping when the text is pure ASCII", () => {
    const text = "const a = 1;\nfunction b() {}\n";
    const table = ByteOffsetTable.for(text);
    assert.strictEqual(table.isIdentity, true);
    assert.strictEqual(table.toByte(0), 0);
    assert.strictEqual(table.toByte(11), 11);
    assert.strictEqual(table.toByte(text.length), text.length);
  });
});

describe("ByteOffsetTable (non-ASCII)", () => {
  const text = 'const é中文 = "😀";';

  it("converts a UTF-16 index past multi-byte characters", () => {
    const table = ByteOffsetTable.for(text);
    assert.strictEqual(table.isIdentity, false);
    assert.strictEqual(table.toByte(6), 6); // é starts here
    assert.strictEqual(table.toByte(7), 8); // after é (2 bytes)
    assert.strictEqual(table.toByte(8), 11); // 中 starts here
    assert.strictEqual(table.toByte(10), 15); // after 文 + the space at 9
    assert.strictEqual(table.toByte(13), 18); // 😀 high surrogate starts
    assert.throws(() => table.toByte(14), /surrogate/); // low surrogate: interior
    assert.strictEqual(table.toByte(15), 22); // after 😀 (4 bytes)
    assert.strictEqual(
      table.toByte(text.length),
      Buffer.byteLength(text, "utf8")
    );
  });

  it("matches Buffer.byteLength over a BMP-only file", () => {
    const text = 'x = "ééé" + "中文中文";';
    const table = ByteOffsetTable.for(text);
    assert.strictEqual(
      table.toByte(text.length),
      Buffer.byteLength(text, "utf8")
    );
  });

  it("does not fail on an endpoint at the pair's high start or after it", () => {
    const text = "a😀b";
    const table = ByteOffsetTable.for(text);
    assert.strictEqual(table.toByte(1), 1);
    assert.strictEqual(table.toByte(3), 5); // after the 4-byte character
  });
});
