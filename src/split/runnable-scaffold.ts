/**
 * Runnable scaffolding for a runnable --split output tree.
 *
 * The emitted graph loads and runs under Node, but a Bun bundle leaves two
 * loose ends between "loads" and "runs standalone":
 *   - a few packages Bun kept external (`require("ws")`, `require("ajv")`)
 *     that must come from node_modules, and
 *   - `using`/`await using` declarations that older Node cannot parse.
 *
 * This writes a self-contained runner (`run.cjs`) that boots the entry —
 * running `using` faithfully (native on Node >= 24, else re-exec under V8's
 * explicit-resource-management flag so disposal still fires; loud failure
 * only when neither is possible) — plus a `package.json` listing the detected
 * externals (so `npm install` provisions them) and a short `RUNNABLE.md`. The
 * result: an unpacked humanified tree you can `npm install && node run.cjs`
 * and actually run.
 */

import { readFileSync } from "node:fs";
import { readdir, readFile, writeFile } from "node:fs/promises";
import { builtinModules } from "node:module";
import * as path from "node:path";
import { METADATA_DIR } from "./layout.js";

export const RUNNER_FILENAME = "run.cjs";
export const SCAFFOLD_README = "RUNNABLE.md";

const BUILTINS = new Set<string>(builtinModules);

/** Bare require/import specifiers, capturing the specifier. */
const SPECIFIER_RE = /(?:require|import)\s*\(\s*["']([^"']+)["']\s*\)/g;

function isBuiltin(spec: string): boolean {
  const bare = spec.startsWith("node:") ? spec.slice(5) : spec;
  // `fs/promises`, `stream/web`, … are builtin subpaths.
  return BUILTINS.has(bare) || BUILTINS.has(bare.split("/")[0]);
}

/** The installable package name for a specifier: the scope+name, dropping
 * any deep subpath (`ajv/dist/x` → `ajv`, `@s/p/q` → `@s/p`). */
function packageName(spec: string): string {
  const parts = spec.split("/");
  return spec.startsWith("@") ? parts.slice(0, 2).join("/") : parts[0];
}

/**
 * The external npm packages required across `contents` — bare specifiers
 * that are neither Node builtins nor relative/absolute paths — as a sorted,
 * de-duplicated list of installable package names.
 */
export function externalPackagesFrom(contents: Iterable<string>): string[] {
  const found = new Set<string>();
  for (const text of contents) {
    for (const m of text.matchAll(SPECIFIER_RE)) {
      const spec = m[1];
      if (spec.startsWith(".") || spec.startsWith("/")) continue;
      // A scheme prefix (node:, bun:, http:, data:, …) is a runtime builtin
      // or URL, never an installable npm package. Bun-only modules like
      // `bun:jsc` are guarded behind `typeof Bun` checks at runtime.
      if (spec.includes(":")) continue;
      if (isBuiltin(spec)) continue;
      found.add(packageName(spec));
    }
  }
  return [...found].sort();
}

async function jsFilesUnder(dir: string): Promise<string[]> {
  const out: string[] = [];
  const walk = async (d: string): Promise<void> => {
    for (const entry of await readdir(d, { withFileTypes: true })) {
      const full = path.join(d, entry.name);
      if (entry.isDirectory()) {
        // node_modules is external; .humanify/ is generated metadata (incl.
        // the whole-bundle humanified.js) — neither is part of the graph
        // whose external requires we install.
        if (entry.name === "node_modules" || entry.name === METADATA_DIR) {
          continue;
        }
        await walk(full);
      } else if (entry.name.endsWith(".js") || entry.name.endsWith(".cjs")) {
        out.push(full);
      }
    }
  };
  await walk(dir);
  return out;
}

/** Scan an emitted tree on disk for the external packages it requires. */
export async function detectExternalPackages(dir: string): Promise<string[]> {
  const files = await jsFilesUnder(dir);
  const contents = await Promise.all(files.map((f) => readFile(f, "utf-8")));
  return externalPackagesFrom(contents);
}

/** The `version` of `pkg` as installed nearest `fromDir`, or undefined if it
 * cannot be resolved. Mirrors Node's resolution: check `fromDir/node_modules`,
 * then walk up parent directories (nearest node_modules wins). */
function installedVersion(pkg: string, fromDir: string): string | undefined {
  let dir = path.resolve(fromDir);
  for (;;) {
    const manifest = path.join(dir, "node_modules", pkg, "package.json");
    try {
      const version = JSON.parse(readFileSync(manifest, "utf-8"))?.version;
      if (typeof version === "string") return version;
    } catch {
      // Not installed here — keep walking up.
    }
    const parent = path.dirname(dir);
    if (parent === dir) return undefined;
    dir = parent;
  }
}

/**
 * Map each external to a package.json dependency range. Pins to the exact
 * version installed nearest `fromDir` (the input bundle's directory) — faithful
 * to what the bundle was built against, which matters for packages whose
 * internal layout is version-specific (e.g. `ajv/dist/runtime/*`). Falls back
 * to `"*"` when a version can't be resolved (no node_modules beside the input).
 */
export function resolveExternalVersions(
  externals: string[],
  fromDir: string | undefined
): Record<string, string> {
  const deps: Record<string, string> = {};
  for (const name of externals) {
    const version = fromDir ? installedVersion(name, fromDir) : undefined;
    deps[name] = version ?? "*";
  }
  return deps;
}

function runnerSource(entryFile: string): string {
  return `#!/usr/bin/env node
// Auto-generated by humanify --split. Boots the unpacked tree:
//   node ${RUNNER_FILENAME} --version
// Run \`npm install\` here first if this tree requires external packages.
const Module = require("node:module");
const path = require("node:path");

// \`using\`/\`await using\` (explicit resource management) is syntax the source
// bundle may use. Run it *faithfully* — with real Symbol.dispose /
// Symbol.asyncDispose cleanup — rather than rewriting it to \`const\`, which
// would silently drop disposal and leak the acquired resources:
//   0. under Bun, skip the guard entirely: Bun's file loader parses \`using\`
//      natively even though its eval/new Function rejects it, so the probe
//      below false-negatives — and hooking Module.prototype._compile (the
//      strip path) breaks Bun's CJS loader outright;
//   1. if this Node parses \`using\` natively (Node >= 24), just run it;
//   2. else re-exec once under V8's --js-explicit-resource-management flag,
//      which enables it (with full disposal) on Node that has the flag;
//   3. else (Node too old for even the flag) fail loudly — unless the caller
//      opts into a lossy strip via HUMANIFY_STRIP_USING=1.
const IS_BUN = typeof Bun !== "undefined";
const REEXEC_GUARD = "__HUMANIFY_USING_REEXEC";
function usingParses() {
  try {
    new Function(
      "async function _(){ using x = { [Symbol.dispose](){} }; await using y = { async [Symbol.asyncDispose](){} }; }"
    );
    return true;
  } catch {
    return false;
  }
}
if (!IS_BUN && !usingParses()) {
  if (process.env[REEXEC_GUARD] !== "1") {
    const { spawnSync } = require("node:child_process");
    const res = spawnSync(
      process.execPath,
      [
        "--js-explicit-resource-management",
        __filename,
        ...process.argv.slice(2)
      ],
      { stdio: "inherit", env: { ...process.env, [REEXEC_GUARD]: "1" } }
    );
    process.exit(res.status == null ? 1 : res.status);
  } else if (process.env.HUMANIFY_STRIP_USING === "1") {
    console.warn(
      "[humanify] Stripping \`using\`/\`await using\` — resources acquired via " +
        "\`using\` will NOT be disposed (HUMANIFY_STRIP_USING=1)."
    );
    const compile = Module.prototype._compile;
    Module.prototype._compile = function (content, filename) {
      const stripped = content
        .replace(/\\bawait\\s+using\\b/g, "const")
        .replace(/\\busing\\b(?=\\s+[A-Za-z_$])/g, "const");
      return compile.call(this, stripped, filename);
    };
  } else {
    console.error(
      "[humanify] This tree uses \`using\`/\`await using\` (explicit resource " +
        "management), which this Node cannot parse — even with " +
        "--js-explicit-resource-management. Run it on Node >= 24 or under " +
        "Bun (both support it natively), or set HUMANIFY_STRIP_USING=1 to " +
        "strip them (disposal semantics are lost)."
    );
    process.exit(1);
  }
}

const entry = path.join(__dirname, ${JSON.stringify(entryFile)});
process.argv = [process.argv[0], entry, ...process.argv.slice(2)];
require(entry);
`;
}

function packageJsonSource(dependencies: Record<string, string>): string {
  return `${JSON.stringify(
    {
      name: "humanified-runnable",
      private: true,
      description: "Unpacked, humanified, split bundle — runnable under Node.",
      scripts: { start: `node ${RUNNER_FILENAME}` },
      dependencies
    },
    null,
    2
  )}\n`;
}

function readmeSource(
  entryFile: string,
  dependencies: Record<string, string>
): string {
  const names = Object.keys(dependencies);
  const pinned = names.filter((n) => dependencies[n] !== "*").length;
  const versionNote =
    pinned === names.length
      ? "Versions are pinned to the copies installed beside the input bundle.\n"
      : pinned > 0
        ? `${pinned} of ${names.length} version(s) are pinned to the copies installed ` +
          'beside the input bundle; the rest use "*" — pin them if a package\'s ' +
          "internal layout must match the original bundle.\n"
        : 'Versions are best-effort ("*"); pin them if a package\'s internal ' +
          "layout must match the original bundle.\n";
  const deps = names.length
    ? `This tree requires ${names.length} external package(s): ${names.join(", ")}.\n` +
      versionNote +
      "\n```sh\nnpm install\n```\n\n"
    : "This tree needs no external packages.\n\n";
  return `# Running this tree

${deps}Then boot it:

\`\`\`sh
node ${RUNNER_FILENAME} --version
node ${RUNNER_FILENAME} --help
\`\`\`

\`${RUNNER_FILENAME}\` loads \`${entryFile}\`, which requires every module in
the tree (split runtime files + re-linked Bun factory modules). It runs
\`using\`/\`await using\` faithfully: natively under Bun or Node >= 24,
otherwise it re-execs once under \`--js-explicit-resource-management\` so
disposal still fires. On Node too old for that flag it stops with an error;
set \`HUMANIFY_STRIP_USING=1\` to strip \`using\` instead (disposal is then
lost).

If the original bundle targets the Bun runtime (calls \`Bun.*\` APIs), run it
under Bun — Node has no \`Bun\` global, so a real workload will stop at the
first such call regardless of syntax support:

\`\`\`sh
bun ${RUNNER_FILENAME} --version
\`\`\`
`;
}

/**
 * Write the runner, package.json, and README into an emitted tree.
 * `resolveFromDir` (the input bundle's directory) is where installed
 * dependency versions are resolved from; omit it to pin everything at `"*"`.
 */
export async function writeRunnableScaffold(
  outputDir: string,
  entryFile: string,
  externals: string[],
  resolveFromDir?: string
): Promise<void> {
  const dependencies = resolveExternalVersions(externals, resolveFromDir);
  await writeFile(
    path.join(outputDir, RUNNER_FILENAME),
    runnerSource(entryFile)
  );
  await writeFile(
    path.join(outputDir, "package.json"),
    packageJsonSource(dependencies)
  );
  await writeFile(
    path.join(outputDir, SCAFFOLD_README),
    readmeSource(entryFile, dependencies)
  );
}
