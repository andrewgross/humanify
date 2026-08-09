/**
 * 051 — the pairing rule inside `classifyFile`, under test.
 *
 * `statementHash` masks EVERY identifier name (its own docstring says short
 * statements "collide across unrelated code"), so "same hash" means "same shape
 * with all names blanked" — NOT "the same statement, renamed". When a file holds
 * several statements of one shape, step 2 pairs them FIFO, and a wrong pairing
 * manufactures a naming instance out of two unrelated statements.
 *
 * These tests pin both rules: FIFO stays the default byte-for-byte (every number
 * in the 033-050 arc came out of it), and the corroborated rule is available to
 * score the same trees a second way.
 *
 *   npx tsx --test experiments/037-noise-source-decomposition/diff-composition.test.ts
 */
import assert from "node:assert";
import { test } from "node:test";
import { composeFile } from "./diff-composition.js";

/** Two statements of identical SHAPE and no literals — hash-identical, so the
 * pairing rule alone decides which prior statement each fresh one is "a rename
 * of". Multi-line so a mispairing costs visible git lines. */
const stmt = (head: string, a: string, b: string, c: string) =>
  `register(${head}, {\n  factory: ${a},\n  policy: ${b},\n  limiter: ${c}\n});`;

const PRIOR = [
  stmt("alphaHandler", "alphaFactory", "alphaPolicy", "alphaLimiter"),
  stmt("betaHandler", "betaFactory", "betaPolicy", "betaLimiter")
].join("\n");

// Both statements edited by one identifier each AND emitted in the other order:
// neither exact-matches, so both land in the same hash bucket and FIFO pairs
// each fresh statement with the WRONG prior one.
const FRESH_SWAPPED = [
  stmt("betaHandler", "betaFactory", "betaPolicy", "betaThrottle"),
  stmt("alphaHandler", "alphaFactory", "alphaPolicy", "alphaGate")
].join("\n");

test("FIFO pairing charges a swapped pair as wholesale naming churn", () => {
  const t = composeFile(PRIOR, FRESH_SWAPPED);
  // Truth is one changed line per statement, i.e. 4 git lines. FIFO pairs
  // alpha-with-beta and bills nearly every line of both.
  assert.ok(
    t.naming >= 12,
    `expected FIFO to over-charge (>=12 ln), got ${t.naming}`
  );
});

test("corroborated pairing charges only the lines that actually changed", () => {
  const t = composeFile(PRIOR, FRESH_SWAPPED, { pairing: "corroborated" });
  assert.strictEqual(t.naming, 4, "one changed line per side, per statement");
  assert.strictEqual(t.real, 0, "nothing here is real change");
});

test("corroborated pairing refuses a same-hash pair with no shared tokens", () => {
  const prior = stmt(
    "alphaHandler",
    "alphaFactory",
    "alphaPolicy",
    "alphaLimiter"
  );
  const fresh = `connect(socketPool, {\n  reader: streamReader,\n  policy: framedCodec,\n  limiter: backpressure\n});`;
  const fifo = composeFile(prior, fresh);
  assert.ok(fifo.naming > 0, "FIFO calls two unrelated statements a rename");
  const corr = composeFile(prior, fresh, { pairing: "corroborated" });
  assert.strictEqual(corr.naming, 0, "refused: not the same statement");
  assert.ok(corr.real > 0, "the lines are real change, and git prints them");
});

test("the default rule is FIFO, unchanged", () => {
  const a = composeFile(PRIOR, FRESH_SWAPPED);
  const b = composeFile(PRIOR, FRESH_SWAPPED, { pairing: "fifo" });
  assert.deepStrictEqual(a, b);
});

test("a file that fails to parse is FATAL, never silently reclassified", () => {
  // statementsOf used to return [] on a parse failure, which converted the
  // entire prior file into "real removed" lines inside the lead KPI — a
  // broken emitted file scoring as a huge genuine change. Fail loud instead
  // (the rule experiments/lib/diff.ts was created to enforce).
  assert.throws(
    () => composeFile("function ok() { return 1; }", "function broken( {{{"),
    /parse/i,
    "unparseable fresh input must throw"
  );
  assert.throws(
    () => composeFile("function broken( {{{", "function ok() { return 1; }"),
    /parse/i,
    "unparseable prior input must throw"
  );
});
