/**
 * The `rust:parity` check stage: the parity differ proven able to fail.
 *
 * `humanify-parity selftest` plants divergences in synthetic dumps and
 * requires every one DETECTED, with the unplanted control identical — the
 * instrument's own proof (docs/rust-port/07-differential-validation.md §4,
 * measurement rule 3: a compare zero means nothing until this has run).
 *
 * Until the cutover this stage also compared committed TS-side fixture dumps
 * (test/parity/<fixture>/ts/). Those were the TS oracle's decision records;
 * the TS pipeline is deleted (docs/rust-port/19-cutover.md), so the differ
 * now compares Rust against Rust — two `--dump-artifacts` dumps of the
 * binary — and this stage keeps it honest. Exit nonzero on any failure.
 */
import { execFileSync } from "node:child_process";

execFileSync(
  "cargo",
  ["run", "-q", "-p", "humanify-parity", "--", "selftest"],
  {
    stdio: "inherit"
  }
);
console.log("  selftest: every planted divergence detected");
