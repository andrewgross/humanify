import assert from "node:assert";
import { execFileSync } from "node:child_process";
import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import { describe, it } from "node:test";

/**
 * The walked CLI's environment hygiene (experiments/lib/boot-gate.sh).
 *
 * A launching agent session exports launcher-specific state the walked CLI
 * must not inherit (boot-gate.sh's header lists the incident history). Two
 * classes have bitten:
 * - MODEL overrides (ANTHROPIC_MODEL + per-tier defaults): 2026-10-02, a
 *   session's GLM model remapped the pinned boot model and failed all four
 *   boots of a real eval;
 * - the ENDPOINT override (ANTHROPIC_BASE_URL) and its auth: 2026-10-27, a
 *   session on a different backend (host.docker.internal:8100) pointed the
 *   walked CLI at an endpoint it could not authenticate to; every boot
 *   failed with "issue with the selected model" and the PINNED model's name
 *   in the message — a red herring. The poisoned eval read plausible churn
 *   numbers before the boot failures were noticed.
 *
 * OBSERVATIONAL (the rule run-launch.test.ts is built on), and shaped like
 * the gate's real use: BOOT_ENV is an ARGUMENT LIST to `env`, so hygiene
 * only happens when the walked CLI is launched THROUGH the array —
 * `boot_version` / `boot_prompt` execute `"${BOOT_ENV[@]}" bun ...`. The
 * probe mirrors that: it sources boot-gate.sh (with a stub `bun` on PATH,
 * because the script's source-time check is fatal without one) and launches
 * a checker script THROUGH "${BOOT_ENV[@]}"; the checker reports every
 * watched variable it inherited. Hermetic: the stub is this test's, no
 * network, no real CLI.
 */

const BOOT_GATE = path.resolve(
  import.meta.dirname,
  "../experiments/lib/boot-gate.sh"
);

const LEAK_CLASS: Record<string, string> = {
  ANTHROPIC_MODEL: "GLM-5.3-Flash-NVFP4",
  ANTHROPIC_BASE_URL: "http://host.docker.internal:8100",
  ANTHROPIC_API_KEY: "sk-fake-do-not-echo",
  ANTHROPIC_AUTH_TOKEN: "tok-fake-do-not-echo",
  ANTHROPIC_DEFAULT_OPUS_MODEL: "GLM-5.3-Flash-NVFP4",
  ANTHROPIC_DEFAULT_SONNET_MODEL: "GLM-5.3-Flash-NVFP4",
  ANTHROPIC_DEFAULT_HAIKU_MODEL: "GLM-5.3-Flash-NVFP4",
  ANTHROPIC_DEFAULT_FABLE_MODEL: "GLM-5.3-Flash-NVFP4",
  CLAUDE_CODE_SUBAGENT_MODEL: "GLM-5.3-Flash-NVFP4",
  CLAUDECODE: "1",
  CLAUDE_CODE_ENTRYPOINT: "cli",
  CLAUDE_CODE_SESSION_ID: "session-fake"
};

const WATCHED = Object.keys(LEAK_CLASS);

/** The checker the probe launches: exits 1 naming any inherited variable. */
const CHECKER_BODY = [
  "#!/bin/bash",
  ...WATCHED.map(
    (v) => `[ -n "\${${v}+set}" ] && { echo "LEAK ${v}"; exit 1; }`
  ),
  'echo "ALL-CLEAN"'
].join("\n");

/** A tree with a stub `bun` on PATH (the gate's source-time check needs one). */
function fakeTree(): string {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "boot-env-"));
  fs.mkdirSync(path.join(dir, "bin"));
  fs.writeFileSync(path.join(dir, "bin", "bun"), "#!/bin/sh\necho 1.4.0\n");
  fs.chmodSync(path.join(dir, "bin", "bun"), 0o755);
  const checker = path.join(dir, "checker.sh");
  fs.writeFileSync(checker, CHECKER_BODY);
  fs.chmodSync(checker, 0o755);
  return dir;
}

describe("boot-gate.sh env hygiene", () => {
  it("a CLI launched through BOOT_ENV inherits none of the launcher's state", () => {
    const tree = fakeTree();
    const probe = path.join(tree, "probe.sh");
    fs.writeFileSync(
      probe,
      [
        // The literal shell text BOOT_ENV[@] — string-built (not a template
        // literal) so the `${...}` shell syntax is not read as a TS
        // template placeholder.
        ["source", BOOT_GATE].join(" "),
        // The env array-expansion, built from pieces so no TS string reads
        // as a template placeholder and useTemplate never fires.
        ['"$' + "{BOOT_ENV[@]}" + '"', [tree, "/checker.sh"].join("")].join(" ")
      ].join("\n")
    );
    fs.chmodSync(probe, 0o755);
    const out = execFileSync("bash", [probe], {
      env: {
        ...process.env,
        PATH: `${path.join(tree, "bin")}:${process.env.PATH}`,
        ...LEAK_CLASS
      },
      encoding: "utf8"
    });
    assert.match(out, /ALL-CLEAN/);
  });

  it("the probe CAN fire: without BOOT_ENV the leak is visible", () => {
    // Validate the test's own counter can fire (the divergence-probe rule):
    // the same checker WITHOUT launching through BOOT_ENV must see the leak.
    const tree = fakeTree();
    const probe = path.join(tree, "probe.sh");
    fs.writeFileSync(probe, [`${tree}/checker.sh`].join("\n"));
    fs.chmodSync(probe, 0o755);
    assert.throws(
      () =>
        execFileSync("bash", [probe], {
          env: {
            ...process.env,
            PATH: `${path.join(tree, "bin")}:${process.env.PATH}`,
            ...LEAK_CLASS
          },
          encoding: "utf8"
        }),
      // execFileSync throws with stdout/stderr attached; the checker's LEAK
      // line (exit 1) is on stdout.
      (e: unknown) =>
        e !== null &&
        typeof e === "object" &&
        "stdout" in e &&
        String((e as { stdout?: unknown }).stdout).includes("LEAK")
    );
  });
});
