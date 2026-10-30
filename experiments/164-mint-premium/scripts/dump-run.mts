/**
 * exp164: run the release binary over the names-mode hop's exact input,
 * prior and config, with the STUB LLM (scripts/lib/stub-llm.ts) — every
 * dump input (claims, restoration applications, the library freeze, Site
 * A's per-statement rows) is LLM-independent, so the stub reproduces them
 * exactly while answering every ask instantly. Writes the diagnostics
 * (with the new eraRestoration block) to the census work dir.
 */
import { spawn } from "node:child_process";
import * as fs from "node:fs";
import { startStubLlm, stubAnswer } from "../../../scripts/lib/stub-llm.js";

const BIN = process.argv[2];
const OUT = "/work/exp164-mint-census/dump-run";
const INPUT =
  "/Users/andrewgross/Development/claude-code-versions/inputs/claude-code-2.1.216/binary-decompiled/src/entrypoints/index.js";
const PRIOR = "/tmp/eval-work/exp161-names-hop/2.1.215-rebased/.humanify/humanified.js";

fs.rmSync(OUT, { recursive: true, force: true });
fs.mkdirSync(OUT, { recursive: true });
fs.writeFileSync(
  `${OUT}/dump.toml`,
  `[llm]
retries = 0
[split]
enabled = true
[diagnostics]
stats_json = "${OUT}/stats.json"
log_file = "${OUT}/run.log"
diagnostics_file = "${OUT}/exp164.diag.json"
[library]
matching = "names"
names_unlocked = true
index = "/work/library-index/cb64c1f5fd507bee7f80efd46e0d01cec405a8b3028a9c05d56e94d7c071ec28"
[library.lookup]
agreement = false
`,
);

const { server, url } = await startStubLlm();
console.log("stub at", url);
const child = spawn(
  BIN,
  [
    INPUT,
    "--config",
    `${OUT}/dump.toml`,
    "--endpoint",
    url,
    "--model",
    "openai/gpt-oss-20b",
    "-o",
    `${OUT}/tree`,
    "--prior-version",
    PRIOR,
    "-vv",
  ],
  { stdio: ["ignore", "inherit", "pipe"], env: { ...process.env, HUMANIFY_API_KEY: "exp164" } },
);
let stderr = "";
child.stderr.on("data", (c) => {
  stderr += c;
});
const code = await new Promise<number | null>((resolve) => child.on("close", resolve));
server.close();
fs.writeFileSync(`${OUT}/stderr.txt`, stderr);
console.log("binary exit", code, "stderr bytes", stderr.length);
if (code !== 0) console.error(stderr.slice(-3000));
