/**
 * The `rust:parity` check stage (docs/rust-port/05-rust-toolchain.md §8,
 * wired at WP0.3): the parity differ proven able to fail, then the
 * committed fixture-scale dumps compared.
 *
 * Two things run here, in order:
 *  1. `humanify-parity selftest` — the instrument's own proof (07 §4,
 *     measurement rule 3): planted divergences must be DETECTED. A compare
 *     zero means nothing until this has run.
 *  2. Every committed fixture dump pair under test/parity/ compared
 *     ts-vs-ts — the fixture-positive check. With no fixture dumps yet
 *     (they are cut from the fixture set at WP0.4's oracle freeze) the
 *     stage says so LOUDLY rather than silently comparing nothing: an
 *     exclusion only stays safe while it is visible.
 *
 * Exit nonzero on any failure; the stage is pass/fail, never advisory.
 */
import { execFileSync } from "node:child_process";
import * as fs from "node:fs";
import * as path from "node:path";

const REPO = path.resolve(import.meta.dirname, "..");
const FIXTURE_DIR = path.join(REPO, "test", "parity");

function runSelftest(): void {
  execFileSync(
    "cargo",
    ["run", "-q", "-p", "humanify-parity", "--", "selftest"],
    { stdio: "inherit" }
  );
  console.log("  selftest: every planted divergence detected");
}

function compareFixtures(): void {
  if (!fs.existsSync(FIXTURE_DIR)) {
    console.log(
      "  NOTE: no test/parity/ fixture dumps yet (cut at WP0.4's oracle " +
        "freeze) — selftest only. A fixture dump appearing here without a " +
        "pair is a stage failure, by design."
    );
    return;
  }
  const entries = fs
    .readdirSync(FIXTURE_DIR, { withFileTypes: true })
    .filter((e) => e.isDirectory());
  if (entries.length === 0) {
    console.log(
      "  NOTE: test/parity/ exists but holds no fixture dumps — selftest only."
    );
    return;
  }
  for (const entry of entries) {
    const pairDir = path.join(FIXTURE_DIR, entry.name);
    const sides = fs
      .readdirSync(pairDir, { withFileTypes: true })
      .filter((e) => e.isDirectory())
      .map((e) => e.name)
      .sort();
    if (sides.length !== 2) {
      throw new Error(
        `fixture ${entry.name} must hold exactly two dump dirs (a pair), found: ${sides.join(", ")}`
      );
    }
    execFileSync(
      "cargo",
      [
        "run",
        "-q",
        "-p",
        "humanify-parity",
        "--",
        "compare",
        path.join(pairDir, sides[0]),
        path.join(pairDir, sides[1])
      ],
      { stdio: "inherit" }
    );
    console.log(
      `  fixture ${entry.name}: IDENTICAL (${sides[0]} vs ${sides[1]})`
    );
  }
}

runSelftest();
compareFixtures();
