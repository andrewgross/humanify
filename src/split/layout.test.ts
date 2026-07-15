import assert from "node:assert";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { afterEach, beforeEach, describe, it } from "node:test";
import {
  HUMANIFIED_SOURCE_PATH,
  LEGACY_SPLIT_LEDGER_FILENAME,
  SPLIT_LEDGER_PATH,
  findSplitLedgerPath
} from "./layout.js";

describe("findSplitLedgerPath", () => {
  let dir: string;

  beforeEach(() => {
    dir = fs.mkdtempSync(path.join(os.tmpdir(), "humanify-layout-"));
  });

  afterEach(() => {
    fs.rmSync(dir, { recursive: true, force: true });
  });

  function write(rel: string): string {
    const abs = path.join(dir, rel);
    fs.mkdirSync(path.dirname(abs), { recursive: true });
    fs.writeFileSync(abs, "{}");
    return abs;
  }

  it("finds the ledger as a sibling of a prior .humanify/humanified.js target", () => {
    const prior = write(HUMANIFIED_SOURCE_PATH);
    const ledger = write(SPLIT_LEDGER_PATH); // <dir>/.humanify/split-ledger.json
    // prior is <dir>/.humanify/humanified.js → sibling is that same folder
    assert.strictEqual(findSplitLedgerPath(prior), ledger);
  });

  it("finds the ledger under .humanify/ when the prior file is at the tree root", () => {
    const prior = write("output.js"); // <dir>/output.js
    const ledger = write(SPLIT_LEDGER_PATH);
    assert.strictEqual(findSplitLedgerPath(prior), ledger);
  });

  it("falls back to the legacy flat ledger filename", () => {
    const prior = write("output.js");
    const legacy = write(LEGACY_SPLIT_LEDGER_FILENAME);
    assert.strictEqual(findSplitLedgerPath(prior), legacy);
  });

  it("prefers the sibling ledger over a nested/legacy one", () => {
    const prior = write(HUMANIFIED_SOURCE_PATH);
    const sibling = write(
      `${path.dirname(HUMANIFIED_SOURCE_PATH)}/split-ledger.json`
    );
    write(LEGACY_SPLIT_LEDGER_FILENAME);
    assert.strictEqual(findSplitLedgerPath(prior), sibling);
  });

  it("returns undefined when no ledger exists", () => {
    const prior = write("output.js");
    assert.strictEqual(findSplitLedgerPath(prior), undefined);
  });
});
