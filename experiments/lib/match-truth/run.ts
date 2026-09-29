/**
 * The ground-truth runner (exp092): download (once) the pinned corpus,
 * produce the two input forms the pipeline expects per package pair —
 * the OLD version's shipped single-file build run through the stage-6
 * formatter (`humanify format`, the closest no-LLM equivalent of a
 * humanified prior) as the prior, and the NEW version's shipped build
 * as the input — run `humanify match` over each pair, score each dump,
 * and write one deterministic scorecard JSON.
 *
 * Usage:
 *   npm run match-truth [-- --corpus <dir>] [--bin <path>] [--out <file>] [--only <name>]
 *
 * Defaults: the release binary (target/release/humanify — build it with
 * `cargo build --release --locked -p humanify-cli`), the corpus cache
 * under experiments/092-match-ground-truth/.corpus, results to stdout.
 * No LLM is contacted at any point (matching is fully cold).
 */
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import process from "node:process";

import { CORPUS } from "./corpus.js";
import { aggregate } from "./score.js";
import type { PackageScorecard } from "./score.js";
import { scoreMatchDump } from "./score.js";

interface Flags {
  corpusDir: string;
  bin: string;
  out?: string;
  only?: string;
}

function parseFlags(argv: string[]): Flags {
  const flags: Flags = {
    corpusDir: resolve("experiments/092-match-ground-truth/.corpus"),
    bin: resolve("target/release/humanify")
  };
  for (let i = 0; i < argv.length; i += 1) {
    const a = argv[i];
    if (a === "--corpus") {
      flags.corpusDir = resolve(argv[++i]);
    } else if (a === "--bin") {
      flags.bin = resolve(argv[++i]);
    } else if (a === "--out") {
      flags.out = argv[++i];
    } else if (a === "--only") {
      flags.only = argv[++i];
    } else {
      throw new Error(`unknown flag: ${a}`);
    }
  }
  return flags;
}

function run(
  cmd: string,
  args: string[],
  cwd: string,
  timeoutMs: number
): string {
  return execFileSync(cmd, args, {
    cwd,
    encoding: "utf8",
    timeout: timeoutMs,
    maxBuffer: 64 * 1024 * 1024
  }).toString();
}

/** The tarball for one pinned version, downloaded at most once. */
function tarball(corpusDir: string, pkg: string, version: string): string {
  const path = join(corpusDir, `${pkg}-${version}.tgz`);
  if (!existsSync(path)) {
    console.log(`  downloading ${pkg}@${version}`);
    run(
      "npm",
      [
        "pack",
        `${pkg}@${version}`,
        "--silent",
        "--pack-destination",
        corpusDir
      ],
      corpusDir,
      5 * 60_000
    );
  }
  return path;
}

/** Extract `package/<file>` from the tarball to a stable path. */
function extract(
  corpusDir: string,
  pkg: string,
  version: string,
  file: string
): string {
  const destDir = join(corpusDir, "extracted", pkg, version);
  mkdirSync(destDir, { recursive: true });
  // The cache key carries the in-tarball path (flattened), not just the
  // basename — two corpus entries of one package can share a basename at
  // different paths (bluebird's browser/ vs release/ builds).
  const dest = join(destDir, file.split("/").join("__"));
  if (existsSync(dest)) {
    return dest;
  }
  const tgz = tarball(corpusDir, pkg, version);
  const bytes = run(
    "tar",
    ["-xzf", tgz, "-O", `package/${file}`],
    destDir,
    60_000
  );
  writeFileSync(dest, bytes);
  return dest;
}

function main(): void {
  const flags = parseFlags(process.argv.slice(2));
  if (!existsSync(flags.bin)) {
    throw new Error(
      `the humanify binary is not at ${flags.bin} — build it with \
       \`cargo build --release --locked -p humanify-cli\` or pass --bin`
    );
  }
  mkdirSync(flags.corpusDir, { recursive: true });
  const workDir = resolve("experiments/092-match-ground-truth/.runs");
  mkdirSync(workDir, { recursive: true });
  const binSha = createHash("sha256")
    .update(readFileSync(flags.bin))
    .digest("hex");

  const entries = flags.only
    ? CORPUS.filter((c) => c.package === flags.only)
    : CORPUS;
  if (entries.length === 0) {
    throw new Error(`no corpus entry named ${flags.only}`);
  }

  const packages: PackageScorecard[] = [];
  for (const entry of entries) {
    console.log(
      `== ${entry.package} ${entry.oldVersion} -> ${entry.newVersion}`
    );
    const oldFile = extract(
      flags.corpusDir,
      entry.package,
      entry.oldVersion,
      entry.file
    );
    const newFile = extract(
      flags.corpusDir,
      entry.package,
      entry.newVersion,
      entry.file
    );
    const pairDir = join(workDir, entry.package);
    mkdirSync(pairDir, { recursive: true });
    const priorFile = join(pairDir, "prior.formatted.js");
    run(flags.bin, ["format", oldFile, "-o", priorFile], pairDir, 10 * 60_000);
    const dumpFile = join(pairDir, "dump.json");
    run(
      flags.bin,
      [
        "match",
        newFile,
        "--prior-version",
        priorFile,
        "-o",
        dumpFile,
        "--work-dir",
        join(pairDir, "work")
      ],
      pairDir,
      15 * 60_000
    );
    const scorecard = scoreMatchDump(
      JSON.parse(readFileSync(dumpFile, "utf8"))
    );
    const f = scorecard.functions;
    console.log(
      `   functions: must ${f.matchedOfMust}/${f.mustMatch} ` +
        `(recall ${f.recall === null ? "-" : f.recall.toFixed(3)}), ` +
        `should ${f.matchedOfShould}/${f.shouldMatch}, ` +
        `reported ${f.reported} ` +
        `(identical ${f.reportedClasses.identical}, near ${f.reportedClasses.near}, ` +
        `far ${f.reportedClasses.far})`
    );
    packages.push({
      package: entry.package,
      oldVersion: entry.oldVersion,
      newVersion: entry.newVersion,
      file: entry.file,
      scorecard
    });
  }

  const total = aggregate(packages);
  const result = {
    schemaVersion: 1,
    tool: "humanify match (exp092 ground-truth harness)",
    binary: { path: flags.bin, sha256: binSha },
    corpus: entries,
    total,
    packages
  };
  const json = `${JSON.stringify(result, null, 2)}\n`;
  if (flags.out) {
    writeFileSync(flags.out, json);
    console.log(`scorecard written to ${flags.out}`);
  } else {
    console.log(json);
  }
  // Totals-first reporting: the aggregate line, then the weakest
  // packages (the leads for the noise work).
  console.log(
    `TOTAL ${total.matchedOfMust}/${total.mustMatch} must-match ` +
      `(recall ${total.recall === null ? "-" : total.recall.toFixed(4)}), ` +
      `${total.matchedOfShould}/${total.shouldMatch} should-match, ` +
      `statements ${total.statementProposedOfMust}/${total.statementMustMatch}`
  );
  const weakest = [...packages].sort(
    (a, b) =>
      (a.scorecard.functions.recall ?? 1) - (b.scorecard.functions.recall ?? 1)
  );
  for (const p of weakest.slice(0, 5)) {
    const r = p.scorecard.functions.recall;
    console.log(
      `  lead: ${p.package} recall ${r === null ? "-" : r.toFixed(3)} ` +
        `(${p.scorecard.functions.missedMust.length} missed must-pairs)`
    );
  }
}

main();
