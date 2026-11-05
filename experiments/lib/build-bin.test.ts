import assert from "node:assert";
import { spawnSync } from "node:child_process";
import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import { describe, it } from "node:test";

const SCRIPT = path.join(import.meta.dirname, "build-bin.sh");

/**
 * build_humanify against a fake checkout and a fake cargo (first on PATH as
 * `$HOME/.cargo/bin/cargo`, the PATH entry the script prepends): what it
 * asked cargo to build, what it printed, and its exit code.
 */
function buildIn(crates: string[]): {
  code: number | null;
  stdout: string;
  cargoArgs: string;
} {
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), "build-bin-"));
  try {
    const root = path.join(tmp, "repo");
    for (const c of crates) {
      fs.mkdirSync(path.join(root, "crates", c), { recursive: true });
    }
    fs.mkdirSync(root, { recursive: true });
    const home = path.join(tmp, "home");
    const cargo = path.join(home, ".cargo/bin/cargo");
    fs.mkdirSync(path.dirname(cargo), { recursive: true });
    const record = path.join(tmp, "cargo-args");
    fs.writeFileSync(cargo, `#!/bin/sh\necho "$@" > "${record}"\n`, {
      mode: 0o755
    });
    const r = spawnSync(
      "bash",
      [
        "-c",
        `source "${SCRIPT}" && build_humanify "$1" "$2"`,
        "_",
        root,
        path.join(tmp, "log")
      ],
      { encoding: "utf8", env: { ...process.env, HOME: home } }
    );
    const cargoArgs = fs.existsSync(record)
      ? fs.readFileSync(record, "utf8").trim()
      : "";
    return {
      code: r.status,
      stdout: r.stdout.trim().replace(root, "<ROOT>"),
      cargoArgs
    };
  } finally {
    fs.rmSync(tmp, { recursive: true, force: true });
  }
}

describe("build-bin: the pipeline binary of the commit a leg checked out", () => {
  // exp186: the product is `humanify-ag` (crate humanify-ag-cli). A neutrality
  // baseline built from an older commit still has the `humanify` binary of
  // crate humanify-cli, and must be built and found under THAT name.
  it("a checkout with humanify-ag-cli builds and names target/release/humanify-ag", () => {
    const r = buildIn(["humanify-ag-cli", "humanify-ag-core"]);
    assert.strictEqual(r.code, 0);
    assert.strictEqual(
      r.cargoArgs,
      "build --release --locked -p humanify-ag-cli"
    );
    assert.strictEqual(r.stdout, "<ROOT>/target/release/humanify-ag");
  });

  it("a checkout from before the rename builds and names target/release/humanify", () => {
    const r = buildIn(["humanify-cli", "humanify-core"]);
    assert.strictEqual(r.code, 0);
    assert.strictEqual(r.cargoArgs, "build --release --locked -p humanify-cli");
    assert.strictEqual(r.stdout, "<ROOT>/target/release/humanify");
  });

  it("a checkout with neither pipeline crate fails without building", () => {
    const r = buildIn(["something-else"]);
    assert.notStrictEqual(r.code, 0);
    assert.strictEqual(r.cargoArgs, "");
    assert.strictEqual(r.stdout, "");
  });
});
