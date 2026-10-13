/**
 * DETECTION CENSUS — `humanify detect` over inputs whose bundler is KNOWN,
 * printed as a confusion table (true bundler × detected bundler) plus every
 * miss by name. Reads no bundle into the repo: the corpus and the Claude
 * Code inputs stay where they live; only their paths are passed in.
 *
 *   npx tsx experiments/lib/detect-census.ts --bin target/release/humanify \
 *     [--corpus <corpus.json> --data <dir>] [--cc-inputs <dir> --cc 2.1.86,...]
 *
 * Inputs, each with its truth:
 *   - every committed e2e fixture build (`test/e2e/fixtures/<name>/build/v<v>/
 *     build/index.js`) whose fixture.config.json names `expect.bundler`;
 *   - `--corpus`: an exp096-style corpus.json (`entries[].bundler`, the
 *     bundle at `<data>/corpus/<id>/<files.bundle.name>`);
 *   - `--cc`: Claude Code releases (truth: bun) at
 *     `<cc-inputs>/claude-code-<v>/binary-decompiled/src/entrypoints/index.js`.
 */
import { spawnSync } from "node:child_process";
import * as fs from "node:fs";
import * as path from "node:path";
import { parseArgs } from "node:util";

interface CensusInput {
  id: string;
  file: string;
  truth: string;
}

interface CorpusEntry {
  id: string;
  bundler: string;
  files: { bundle: { name: string } };
}

const FIXTURES = path.join(import.meta.dirname, "../../test/e2e/fixtures");

function fixtureInputs(): CensusInput[] {
  const out: CensusInput[] = [];
  for (const name of fs.readdirSync(FIXTURES).sort()) {
    const cfgPath = path.join(FIXTURES, name, "fixture.config.json");
    if (!fs.existsSync(cfgPath)) continue;
    const truth = JSON.parse(fs.readFileSync(cfgPath, "utf8"))?.expect?.bundler;
    if (typeof truth !== "string") continue;
    const builds = path.join(FIXTURES, name, "build");
    for (const v of fs.readdirSync(builds).sort()) {
      const file = path.join(builds, v, "build/index.js");
      if (fs.existsSync(file))
        out.push({ id: `e2e:${name}@${v}`, file, truth });
    }
  }
  return out;
}

function corpusInputs(corpus: string, data: string): CensusInput[] {
  const entries: CorpusEntry[] = JSON.parse(
    fs.readFileSync(corpus, "utf8")
  ).entries;
  return entries.map((e) => ({
    id: e.id,
    file: path.join(data, "corpus", e.id, e.files.bundle.name),
    truth: e.bundler
  }));
}

function ccInputs(dir: string, versions: string[]): CensusInput[] {
  return versions.map((v) => ({
    id: `cc:${v}`,
    file: path.join(
      dir,
      `claude-code-${v}`,
      "binary-decompiled/src/entrypoints/index.js"
    ),
    truth: "bun"
  }));
}

function detected(bin: string, file: string): string {
  const r = spawnSync(bin, ["detect", file], {
    encoding: "utf8",
    maxBuffer: 64 << 20
  });
  if (r.status !== 0) return `error(${r.status})`;
  const v = JSON.parse(r.stdout).bundler;
  return `${v.type}${v.tier === "definitive" ? "" : `/${v.tier}`}`;
}

function printTable(rows: { truth: string; got: string }[]): void {
  const truths = [...new Set(rows.map((r) => r.truth))].sort();
  const gots = [...new Set(rows.map((r) => r.got))].sort();
  const w = Math.max(10, ...gots.map((g) => g.length)) + 2;
  console.log(
    `${"truth \\ detected".padEnd(18)}${gots.map((g) => g.padStart(w)).join("")}${"total".padStart(8)}`
  );
  for (const t of truths) {
    const mine = rows.filter((r) => r.truth === t);
    const cells = gots.map((g) =>
      String(mine.filter((r) => r.got === g).length).padStart(w)
    );
    console.log(
      `${t.padEnd(18)}${cells.join("")}${String(mine.length).padStart(8)}`
    );
  }
}

function main(): void {
  const { values } = parseArgs({
    options: {
      bin: { type: "string", default: "target/release/humanify" },
      corpus: { type: "string" },
      data: { type: "string" },
      "cc-inputs": { type: "string" },
      cc: { type: "string" }
    },
    strict: true
  });
  const inputs = fixtureInputs();
  if (values.corpus) {
    if (!values.data) throw new Error("--corpus needs --data");
    inputs.push(...corpusInputs(values.corpus, values.data));
  }
  if (values.cc) {
    if (!values["cc-inputs"]) throw new Error("--cc needs --cc-inputs");
    inputs.push(...ccInputs(values["cc-inputs"], values.cc.split(",")));
  }
  const rows = inputs.map((i) => ({ ...i, got: detected(values.bin, i.file) }));
  for (const r of rows) console.log(`${r.id}\t${r.truth}\t${r.got}`);
  console.log();
  printTable(rows);
  const misses = rows.filter((r) => r.got !== r.truth);
  console.log(`\n${rows.length - misses.length}/${rows.length} correct`);
  for (const m of misses) console.log(`  MISS ${m.id}: ${m.truth} -> ${m.got}`);
}

main();
