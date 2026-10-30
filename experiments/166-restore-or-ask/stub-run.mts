/**
 * exp166: run a humanify binary over one bundle in NAMES mode against the
 * STUB LLM (scripts/lib/stub-llm.ts). Every placement-precision input
 * (claims, the era's placement, the freeze, the raw-to-era bridge) is
 * LLM-independent, so the stub reproduces it while answering every ask
 * instantly. Writes the diagnostics file the precision check reads.
 *
 * usage: npx tsx stub-run.mts <bin> <input.js> <out-dir> [<prior.js>] [--mode names|default]
 */
import { spawn } from "node:child_process";
import * as fs from "node:fs";
import { startStubLlm } from "../../scripts/lib/stub-llm.js";

const INDEX =
  "/work/library-index/cb64c1f5fd507bee7f80efd46e0d01cec405a8b3028a9c05d56e94d7c071ec28";
const args = process.argv.slice(2);
const modeAt = args.indexOf("--mode");
const mode = modeAt >= 0 ? args.splice(modeAt, 2)[1] : "names";
const [BIN, INPUT, OUT, PRIOR] = args;
if (!BIN || !INPUT || !OUT) {
  console.error("usage: stub-run.mts <bin> <input.js> <out-dir> [<prior.js>] [--mode names|default]");
  process.exit(2);
}

fs.rmSync(OUT, { recursive: true, force: true });
fs.mkdirSync(OUT, { recursive: true });
const library =
  mode === "names"
    ? `[library]
matching = "names"
names_unlocked = true
index = "${INDEX}"
[library.lookup]
agreement = false
`
    : "";
fs.writeFileSync(
  `${OUT}/run.toml`,
  `[llm]
retries = 0
[split]
enabled = true
[diagnostics]
log_file = "${OUT}/run.log"
diagnostics_file = "${OUT}/diag.json"
${library}`,
);

const { server, url } = await startStubLlm();
const argv = [INPUT, "--config", `${OUT}/run.toml`, "--endpoint", url, "--model", "stub", "-o", `${OUT}/tree`];
if (PRIOR) argv.push("--prior-version", PRIOR);
const child = spawn(BIN, argv, {
  stdio: ["ignore", "ignore", "pipe"],
  env: { ...process.env, HUMANIFY_API_KEY: "exp166" },
});
let stderr = "";
child.stderr.on("data", (c) => {
  stderr += c;
});
const code = await new Promise<number | null>((resolve) => child.on("close", resolve));
server.close();
fs.writeFileSync(`${OUT}/stderr.txt`, stderr);
console.log(`${OUT}: exit ${code}`);
if (code !== 0) console.error(stderr.slice(-2000));
process.exit(code ?? 1);
