/**
 * Execution-count census: which src/ functions ran in production vs only in
 * tests vs never. Detector for runtime-dead code that static import-graph
 * tools cannot see (statically reachable, executed zero times in production,
 * kept warm by tests).
 *
 * Usage: tsx scripts/exec-census.ts <prodCoverageDir> <testCoverageDir>
 * where each dir is a NODE_V8_COVERAGE output directory.
 *
 * CAVEAT: tsx transpiles in-memory; attribution maps V8 offsets back through
 * the inline source maps Node captured in each coverage file's
 * `source-map-cache`. Functions that cannot be attributed are counted and
 * reported as UNATTRIBUTED, never silently dropped. One production run cannot
 * condemn rare paths: error/edge handlers may legitimately show zero.
 */
import { readdirSync, readFileSync } from "node:fs";
import path from "node:path";

type V8Range = { startOffset: number; endOffset: number; count: number };
type V8Fn = { functionName: string; ranges: V8Range[] };
type ScriptCov = { url: string; functions: V8Fn[] };
type SmCacheEntry = { lineLengths: number[]; data: { mappings: string } };
type CoverageFile = {
  result: ScriptCov[];
  "source-map-cache"?: Record<string, SmCacheEntry>;
};
type SourceMap = { lineStarts: number[]; lines: [number, number][][] };
type FnRecord = { file: string; line: number; name: string; count: number };

const REPO = process.cwd();
const B64 = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

function srcRelPath(url: string): string | null {
  if (!url.startsWith("file://")) return null;
  const p = url.slice("file://".length);
  if (p.includes("/node_modules/")) return null;
  if (!p.startsWith(`${REPO}/src/`)) return null;
  const rel = path.relative(REPO, p);
  if (/\.(test|e2etest|fptest)\.ts$/.test(rel)) return null;
  return rel;
}

/** Decode one base64-VLQ value from s starting at pos.i; advances pos.i. */
function decodeVlq(s: string, pos: { i: number }): number {
  let result = 0;
  let shift = 0;
  for (;;) {
    const digit = B64.indexOf(s[pos.i++]);
    result += (digit & 31) << shift;
    if ((digit & 32) === 0) break;
    shift += 5;
  }
  const negative = (result & 1) === 1;
  result >>>= 1;
  return negative ? -result : result;
}

/** Parse a source map's `mappings` into per-generated-line [genCol, origLine]. */
function parseMappings(mappings: string): [number, number][][] {
  const lines: [number, number][][] = [];
  let origLine = 0;
  for (const lineStr of mappings.split(";")) {
    const segs: [number, number][] = [];
    let genCol = 0;
    for (const seg of lineStr.split(",")) {
      if (seg === "") continue;
      const pos = { i: 0 };
      genCol += decodeVlq(seg, pos);
      if (pos.i < seg.length) {
        decodeVlq(seg, pos); // source index (single-source maps)
        origLine += decodeVlq(seg, pos);
        decodeVlq(seg, pos); // original column
        segs.push([genCol, origLine]);
      }
    }
    lines.push(segs);
  }
  return lines;
}

function buildSourceMap(entry: SmCacheEntry): SourceMap {
  const lineStarts: number[] = [0];
  let acc = 0;
  for (const len of entry.lineLengths) {
    acc += len + 1; // +1 for the newline
    lineStarts.push(acc);
  }
  return { lineStarts, lines: parseMappings(entry.data.mappings) };
}

/** Map a generated-code offset to a 1-based original line, or null. */
function originalLine(map: SourceMap, offset: number): number | null {
  let genLine = map.lineStarts.findIndex((s) => s > offset) - 1;
  if (genLine < -1) genLine = map.lineStarts.length - 1;
  if (genLine < 0) return null;
  const genCol = offset - map.lineStarts[genLine];
  for (let ln = genLine; ln >= 0; ln--) {
    const segs = map.lines[ln] ?? [];
    for (let i = segs.length - 1; i >= 0; i--) {
      if (ln < genLine || segs[i][0] <= genCol) return segs[i][1] + 1;
    }
  }
  return null;
}

function listCoverageFiles(dir: string): string[] {
  return readdirSync(dir)
    .filter((f) => f.startsWith("coverage-") && f.endsWith(".json"))
    .map((f) => path.join(dir, f));
}

/** First pass over both dirs: collect url -> SourceMap. */
function collectSourceMaps(files: string[]): Map<string, SourceMap> {
  const maps = new Map<string, SourceMap>();
  for (const file of files) {
    const cov = JSON.parse(readFileSync(file, "utf8")) as CoverageFile;
    for (const [url, entry] of Object.entries(cov["source-map-cache"] ?? {})) {
      if (srcRelPath(url) && !maps.has(url))
        maps.set(url, buildSourceMap(entry));
    }
  }
  return maps;
}

/** fns keyed by `${file}|${startOffset}`; unattributed keyed the same, value = name. */
type DirCensus = {
  fns: Map<string, FnRecord>;
  unattributed: Map<string, string>;
};

/** Aggregate per-function max counts for one coverage dir. */
function censusDir(dir: string, maps: Map<string, SourceMap>): DirCensus {
  const census: DirCensus = { fns: new Map(), unattributed: new Map() };
  for (const file of listCoverageFiles(dir)) {
    const cov = JSON.parse(readFileSync(file, "utf8")) as CoverageFile;
    for (const script of cov.result) {
      const rel = srcRelPath(script.url);
      if (rel) tallyScript(script, rel, maps, census);
    }
  }
  return census;
}

function tallyScript(
  script: ScriptCov,
  rel: string,
  maps: Map<string, SourceMap>,
  census: DirCensus
): void {
  const map = maps.get(script.url);
  for (const fn of script.functions) {
    const start = fn.ranges[0].startOffset;
    if (fn.functionName === "" && start === 0) continue; // module top-level
    const count = Math.max(...fn.ranges.map((r) => r.count));
    const line = map ? originalLine(map, start) : null;
    const key = `${rel}|${start}`;
    if (line === null) {
      census.unattributed.set(key, fn.functionName || "(anon)");
      continue;
    }
    const prev = census.fns.get(key);
    if (prev) prev.count = Math.max(prev.count, count);
    else
      census.fns.set(key, {
        file: rel,
        line,
        name: fn.functionName || "(anon)",
        count
      });
  }
}

function nameHistogram(unattributed: Map<string, string>): string {
  const tally = new Map<string, number>();
  for (const name of unattributed.values())
    tally.set(name, (tally.get(name) ?? 0) + 1);
  return [...tally.entries()]
    .sort((a, b) => b[1] - a[1])
    .slice(0, 6)
    .map(([n, c]) => `${n} x${c}`)
    .join(", ");
}

function classify(prod: DirCensus, test: DirCensus) {
  const neverRan: FnRecord[] = [];
  const testOnly: FnRecord[] = [];
  const alive: FnRecord[] = [];
  const keys = new Set([...prod.fns.keys(), ...test.fns.keys()]);
  for (const key of keys) {
    const p = prod.fns.get(key);
    const t = test.fns.get(key);
    const rec = p ?? t;
    if (!rec) continue;
    if ((p?.count ?? 0) > 0) alive.push(rec);
    else if ((t?.count ?? 0) > 0) testOnly.push(rec);
    else neverRan.push(rec);
  }
  const byPos = (a: FnRecord, b: FnRecord) =>
    a.file === b.file ? a.line - b.line : a.file.localeCompare(b.file);
  return {
    neverRan: neverRan.sort(byPos),
    testOnly: testOnly.sort(byPos),
    alive: alive.sort(byPos)
  };
}

function printSection(title: string, records: FnRecord[]): void {
  console.log(`\n=== ${title}: ${records.length} functions ===`);
  for (const r of records) console.log(`  ${r.file}:${r.line} ${r.name}`);
}

function main(): void {
  const [prodDir, testDir] = process.argv.slice(2);
  if (!prodDir || !testDir) {
    console.error("Usage: tsx scripts/exec-census.ts <prodDir> <testDir>");
    process.exit(2);
  }
  console.log(
    "CAVEAT: tsx transpiles in-memory; offsets are mapped back through the\n" +
      "source-map-cache Node captured per coverage file. Unmappable functions\n" +
      "are counted as UNATTRIBUTED below, never silently dropped. *.test.ts /\n" +
      "*.fptest.ts / *.e2etest.ts files are excluded from the census."
  );
  const allFiles = [
    ...listCoverageFiles(prodDir),
    ...listCoverageFiles(testDir)
  ];
  const maps = collectSourceMaps(allFiles);
  const prod = censusDir(prodDir, maps);
  const test = censusDir(testDir, maps);
  const { neverRan, testOnly, alive } = classify(prod, test);
  console.log(
    `\nTOTAL functions seen: ${neverRan.length + testOnly.length + alive.length}`
  );
  console.log(`  ALIVE (ran in production): ${alive.length}`);
  console.log(
    `  TEST-ONLY (ran in tests, 0 in production): ${testOnly.length}`
  );
  console.log(`  NEVER-RAN (0 in both): ${neverRan.length}`);
  console.log(
    `  UNATTRIBUTED (unique fn sites): prod ${prod.unattributed.size} ` +
      `[${nameHistogram(prod.unattributed)}], tests ${test.unattributed.size} ` +
      `[${nameHistogram(test.unattributed)}]`
  );
  printSection("NEVER-RAN (0 in production AND 0 in tests)", neverRan);
  printSection("TEST-ONLY (scheduler-pattern suspects)", testOnly);
  printSection("ALIVE (ran in production)", alive);
}

main();
