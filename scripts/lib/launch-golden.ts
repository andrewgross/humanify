/**
 * Launch goldens: what a living launcher launches, held byte for byte
 * (exp191; docs/dev-harness.md §4 proof c). The goldens live in
 * experiments/034-eval-harness/launch-goldens/. Most are recorded by
 * test/launch-goldens.test.ts from a sandbox; the e2e one is recorded by
 * the e2e stage itself (scripts/e2e.ts logs every launch it makes), because
 * it needs the release binary and only that stage runs after rust:build.
 *
 * Every golden is a log from a LIVE run, never typed. `UPDATE_LAUNCH_GOLDENS=1`
 * re-records (from a live run like the one checking it — never by hand).
 */
import * as fs from "node:fs";
import * as path from "node:path";

const GOLDENS = path.resolve(
  import.meta.dirname,
  "../../experiments/034-eval-harness/launch-goldens"
);

export function goldenFile(name: string): string {
  return path.join(GOLDENS, `${name}.golden.txt`);
}

/**
 * The golden a recording is held to. With `UPDATE_LAUNCH_GOLDENS=1` the
 * recording is written first, so the comparison passes and the file is the
 * live log.
 */
export function goldenOf(name: string, recorded: string): string {
  const file = goldenFile(name);
  if (process.env.UPDATE_LAUNCH_GOLDENS === "1") {
    fs.mkdirSync(GOLDENS, { recursive: true });
    fs.writeFileSync(file, recorded);
  }
  if (!fs.existsSync(file)) return `<no golden at ${file}>`;
  return fs.readFileSync(file, "utf8");
}

/** The first line where two logs differ, for a failure message. */
export function firstDifference(actual: string, expected: string): string {
  const a = actual.split("\n");
  const e = expected.split("\n");
  for (let i = 0; i < Math.max(a.length, e.length); i++) {
    if (a[i] !== e[i]) {
      return `line ${i + 1}:\n  golden: ${e[i] ?? "<end>"}\n  now:    ${a[i] ?? "<end>"}`;
    }
  }
  return "no line differs";
}

const SAFE = /^[A-Za-z0-9_./:=+@%-]$/;

/**
 * One argument as bash's `printf %q` writes it (bash 5.2): the recorder
 * shells log argv that way, so a log written from TS matches theirs byte
 * for byte. `~` and `#` are quoted only first; non-ASCII passes through.
 * test/launch-golden.test.ts holds it to the real bash.
 */
export function shellQuote(arg: string): string {
  if (arg === "") return "''";
  let out = "";
  for (const [i, ch] of [...arg].entries()) {
    const code = ch.codePointAt(0) ?? 0;
    if (code < 0x20 || code === 0x7f) {
      throw new Error(
        `shellQuote: control character in ${JSON.stringify(arg)}`
      );
    }
    const safe =
      SAFE.test(ch) || code > 0x7f || (i > 0 && (ch === "~" || ch === "#"));
    out += safe ? ch : `\\${ch}`;
  }
  return out;
}
