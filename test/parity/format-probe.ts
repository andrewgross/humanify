/**
 * WP5.6 (5b-1) formatter probe: the TS beautify the native `core::format`
 * must reproduce byte for byte.
 *
 *   npx tsx test/parity/format-probe.ts files <none|full> <pairs.tsv>
 *     each line `<in>\t<out>`: writes the formatted text to <out>
 *     ("none" = transformWithPlugins(code, []), the G1 printer-only leg;
 *      "full" = createBabelPlugin()(code), the stage-6 beautify, G2).
 *     A Babel throw writes `<out>.error` with the message instead.
 *   npx tsx test/parity/format-probe.ts snippets <cases.json>
 *     cases = [{ name, code }]; prints [{ name, code, none, full }] as JSON
 *     (a throw records `{ error }` for that leg) — the committed goldens.
 */
import * as fs from "node:fs";
import { transformWithPlugins } from "../../src/babel-utils.js";
import { createBabelPlugin } from "../../src/plugins/babel/babel.js";

type Leg = { text: string } | { error: string };

const full = createBabelPlugin();

async function run(mode: string, code: string): Promise<Leg> {
  try {
    const text =
      mode === "none" ? await transformWithPlugins(code, []) : await full(code);
    return { text };
  } catch (err) {
    return { error: String((err as Error).message ?? err) };
  }
}

async function files(mode: string, pairsPath: string): Promise<void> {
  const lines = fs.readFileSync(pairsPath, "utf8").split("\n");
  for (const line of lines) {
    if (!line) continue;
    const [input, output] = line.split("\t");
    const leg = await run(mode, fs.readFileSync(input, "utf8"));
    if ("text" in leg) fs.writeFileSync(output, leg.text);
    else fs.writeFileSync(`${output}.error`, leg.error);
  }
}

async function snippets(casesPath: string): Promise<void> {
  const cases = JSON.parse(fs.readFileSync(casesPath, "utf8")) as {
    name: string;
    code: string;
  }[];
  const out = [];
  for (const c of cases) {
    out.push({
      name: c.name,
      code: c.code,
      none: await run("none", c.code),
      full: await run("full", c.code)
    });
  }
  process.stdout.write(`${JSON.stringify(out, null, 2)}\n`);
}

const [verb, a, b] = process.argv.slice(2);
if (verb === "files" && a && b) await files(a, b);
else if (verb === "snippets" && a) await snippets(a);
else {
  process.stderr.write(
    "usage: format-probe.ts files <none|full> <pairs.tsv> | snippets <cases.json>\n"
  );
  process.exit(2);
}
