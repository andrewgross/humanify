/**
 * exp171: measure what the DEFAULT run's library index costs — the
 * run-time find-or-build (exp154) a names-mode run does when no index is
 * passed — on one bundle, against the stub LLM (scripts/lib/stub-llm.ts;
 * the index build and the placement are LLM-independent).
 *
 * Records, per run: the binary's wall clock, the PEAK resident set of the
 * whole process tree (the binary plus the npm / bun children the build
 * spawns, sampled once a second from /proc — GNU time is not installed
 * here), the store's size on disk after the run, and the library report's
 * `indexSource` trail (found vs built, seeds, versions, per-stage times).
 *
 * usage: npx tsx measure-index.mts <bin> <input.js> <out-dir> <store-dir> [--index <dir>] [--mode names|off] [--pre-flip]
 *   --index pins an explicit index instead (the eval's configuration).
 */
import { spawn, spawnSync } from "node:child_process";
import * as fs from "node:fs";
import { startStubLlm } from "../../scripts/lib/stub-llm.js";

const args = process.argv.slice(2);
const flag = (name: string): string | undefined => {
  const at = args.indexOf(name);
  return at >= 0 ? args.splice(at, 2)[1] : undefined;
};
const index = flag("--index");
const mode = flag("--mode") ?? "names";
// --pre-flip: the binary predates exp171 (names behind its unlock, the
// machinery behind index_build = "auto") — the build itself is the same code.
const preFlipAt = args.indexOf("--pre-flip");
const preFlip = preFlipAt >= 0 && args.splice(preFlipAt, 1).length > 0;
const [BIN, INPUT, OUT, STORE] = args;
if (!BIN || !INPUT || !OUT || !STORE) {
  console.error(
    "usage: measure-index.mts <bin> <input.js> <out-dir> <store-dir> [--index <dir>] [--mode names|off]"
  );
  process.exit(2);
}

fs.rmSync(OUT, { recursive: true, force: true });
fs.mkdirSync(OUT, { recursive: true });
const library = [
  "[library]",
  `matching = "${mode}"`,
  ...(mode === "off" ? [] : [`index_store_dir = "${STORE}"`]),
  ...(preFlip && mode === "names" ? ["names_unlocked = true"] : []),
  ...(preFlip && mode !== "off" && !index ? ['index_build = "auto"'] : []),
  ...(index ? [`index = "${index}"`] : [])
].join("\n");
fs.writeFileSync(
  `${OUT}/run.toml`,
  `[llm]
retries = 0
[split]
enabled = true
[diagnostics]
log_file = "${OUT}/run.log"
diagnostics_file = "${OUT}/diag.json"
stats_json = "${OUT}/stats.json"
${library}
`
);

/** Every pid in the tree under `root` (from /proc's ppid links). */
function tree(root: number): number[] {
  const kids = new Map<number, number[]>();
  for (const d of fs.readdirSync("/proc")) {
    if (!/^\d+$/.test(d)) continue;
    try {
      const stat = fs.readFileSync(`/proc/${d}/stat`, "utf8");
      const ppid = Number(stat.slice(stat.lastIndexOf(")") + 2).split(" ")[1]);
      kids.set(ppid, [...(kids.get(ppid) ?? []), Number(d)]);
    } catch {
      /* the process exited between the listing and the read */
    }
  }
  const out: number[] = [];
  const queue = [root];
  while (queue.length > 0) {
    const p = queue.pop() as number;
    out.push(p);
    queue.push(...(kids.get(p) ?? []));
  }
  return out;
}

/** VmRSS of one pid in MB (0 when it has exited). */
function rssMb(pid: number): number {
  try {
    const m = /VmRSS:\s+(\d+) kB/.exec(
      fs.readFileSync(`/proc/${pid}/status`, "utf8")
    );
    return m ? Number(m[1]) / 1024 : 0;
  } catch {
    return 0;
  }
}

/** VmHWM of one pid in MB (its own high-water mark). */
function hwmMb(pid: number): number {
  try {
    const m = /VmHWM:\s+(\d+) kB/.exec(
      fs.readFileSync(`/proc/${pid}/status`, "utf8")
    );
    return m ? Number(m[1]) / 1024 : 0;
  } catch {
    return 0;
  }
}

const { server, url } = await startStubLlm();
const argv = [
  INPUT,
  "--config",
  `${OUT}/run.toml`,
  "--endpoint",
  url,
  "--model",
  "stub",
  "-o",
  `${OUT}/tree`
];
const started = Date.now();
const child = spawn(BIN, argv, {
  stdio: ["ignore", "ignore", "pipe"],
  env: { ...process.env, HUMANIFY_API_KEY: "exp171" }
});
let stderr = "";
child.stderr.on("data", (c) => {
  stderr += c;
});
let peakTreeMb = 0;
let binHwmMb = 0;
const sampler = setInterval(() => {
  const pid = child.pid;
  if (pid === undefined) return;
  const total = tree(pid).reduce((sum, p) => sum + rssMb(p), 0);
  peakTreeMb = Math.max(peakTreeMb, total);
  binHwmMb = Math.max(binHwmMb, hwmMb(pid));
}, 1000);
const code = await new Promise<number | null>((resolve) =>
  child.on("close", resolve)
);
clearInterval(sampler);
server.close();
const wallSeconds = (Date.now() - started) / 1000;
fs.writeFileSync(`${OUT}/stderr.txt`, stderr);

const du = (p: string): number | null => {
  const r = spawnSync("du", ["-sb", p], { encoding: "utf8" });
  return r.status === 0 ? Number(r.stdout.split("\t")[0]) : null;
};
let indexSource: unknown = null;
let libraryProfile: unknown = null;
try {
  const report = JSON.parse(
    fs.readFileSync(`${OUT}/tree/.humanify/library-report.json`, "utf8")
  );
  indexSource = report.indexSource ?? null;
} catch {
  /* names mode writes no library report; the diagnostics carry it */
}
try {
  const stats = JSON.parse(fs.readFileSync(`${OUT}/stats.json`, "utf8"));
  libraryProfile = stats.libraryMatching ?? stats.library ?? null;
} catch {
  /* no stats file (the run failed early) */
}
/** The newest merged index's build.json in the store (the run-time build's
 *  own record: seeds, dropped, versions, built/reused, per-stage times). */
let buildSidecar: unknown = null;
if (fs.existsSync(STORE)) {
  const ids = fs
    .readdirSync(STORE)
    .filter(
      (d) =>
        /^[0-9a-f]{64}$/.test(d) && fs.existsSync(`${STORE}/${d}/build.json`)
    )
    .map((d) => ({ d, t: fs.statSync(`${STORE}/${d}/build.json`).mtimeMs }))
    .sort((a, b) => b.t - a.t);
  if (ids.length > 0 && ids[0].t >= started) {
    buildSidecar = JSON.parse(
      fs.readFileSync(`${STORE}/${ids[0].d}/build.json`, "utf8")
    );
  }
}
const storeSubdirs: Record<string, number | null> = {};
if (fs.existsSync(STORE)) {
  for (const d of fs.readdirSync(STORE)) storeSubdirs[d] = du(`${STORE}/${d}`);
}
const card = {
  input: INPUT,
  mode,
  index: index ?? null,
  store: STORE,
  exitCode: code,
  wallSeconds,
  peakTreeRssMb: Math.round(peakTreeMb),
  binaryPeakRssMb: Math.round(binHwmMb),
  storeBytes: fs.existsSync(STORE) ? du(STORE) : 0,
  storeSubdirs,
  noticeLines: stderr
    .split("\n")
    .filter((l) => /library index|Library index|library names|^  /i.test(l)),
  indexSource,
  buildSidecar,
  libraryProfile
};
fs.writeFileSync(`${OUT}/card.json`, `${JSON.stringify(card, null, 2)}\n`);
console.log(JSON.stringify(card, null, 2));
if (code !== 0) console.error(stderr.slice(-3000));
process.exit(code ?? 1);
