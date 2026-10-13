/**
 * THE writer of the pipeline's `--config` TOML file, for every harness
 * launch (config sweep step 5, 2026-10-13).
 *
 * The binary takes only its operational flags on the command line (the
 * input, `-o`, `--prior-version`, `--endpoint`, `--model`, `--config`,
 * `-v`); every other setting is a key in the config file
 * (`humanify.example.toml` lists each one). So a launcher that used to pass
 * `--split -c 472 --stats-json s.json` now writes
 *
 *   [llm]
 *   concurrency = 472
 *   [split]
 *   enabled = true
 *   [diagnostics]
 *   stats_json = "s.json"
 *
 * and passes `--config <file>`. Settings are given as dotted keys
 * (`split.enabled`), in the order the launcher lists them; the binary
 * checks every key, type and range itself and refuses before any work.
 *
 * CLI (the shell launchers: run.sh, neutrality.sh, walk.sh):
 *   tsx pipeline-config.ts <out.toml> <section.key=value>...   write
 *   tsx pipeline-config.ts --check <section.key=value>...      validate only
 * A value is read as JSON when it parses (`472`, `true`, `["a","b"]`) and
 * as a plain string otherwise (`low`, `/path/to/file`). A key set twice is
 * refused (exit 2): two layers of the harness disagreeing about one
 * setting is exactly the conflict the binary refuses between a flag and
 * the file.
 */
import * as fs from "node:fs";
import * as path from "node:path";

export type Setting = string | number | boolean | string[];

const KEY = /^[a-z_]+(\.[a-z_]+)+$/;

/** `section.key=value` → [key, value]; throws on a malformed entry. */
export function parseSetting(entry: string): [string, Setting] {
  const eq = entry.indexOf("=");
  if (eq < 0) throw new Error(`"${entry}" is not section.key=value`);
  const key = entry.slice(0, eq);
  if (!KEY.test(key)) {
    throw new Error(`"${key}" is not a dotted config key (section.key)`);
  }
  const raw = entry.slice(eq + 1);
  try {
    const v: unknown = JSON.parse(raw);
    if (
      typeof v === "number" ||
      typeof v === "boolean" ||
      typeof v === "string" ||
      (Array.isArray(v) && v.every((x) => typeof x === "string"))
    ) {
      return [key, v as Setting];
    }
  } catch {
    // Not JSON: a plain string.
  }
  return [key, raw];
}

/** Ordered settings, refusing a key set twice. */
export function collectSettings(
  entries: Array<[string, Setting]>
): Map<string, Setting> {
  const out = new Map<string, Setting>();
  for (const [k, v] of entries) {
    if (out.has(k)) {
      throw new Error(
        `${k} is set twice (${JSON.stringify(out.get(k))} and ${JSON.stringify(v)})`
      );
    }
    out.set(k, v);
  }
  return out;
}

/** The TOML text: one table per section, in first-seen order. */
export function renderPipelineConfig(settings: Map<string, Setting>): string {
  const sections = new Map<string, string[]>();
  for (const [key, v] of settings) {
    const dot = key.lastIndexOf(".");
    const section = key.slice(0, dot);
    const lines = sections.get(section) ?? [];
    // JSON strings, numbers and booleans are valid TOML literals.
    const value = Array.isArray(v)
      ? `[${v.map((x) => JSON.stringify(x)).join(", ")}]`
      : JSON.stringify(v);
    lines.push(`${key.slice(dot + 1)} = ${value}`);
    sections.set(section, lines);
  }
  return [...sections]
    .map(([s, lines]) => `[${s}]\n${lines.join("\n")}\n`)
    .join("");
}

export function writePipelineConfig(
  file: string,
  entries: Array<[string, Setting]>
): string {
  const text = renderPipelineConfig(collectSettings(entries));
  fs.writeFileSync(file, text);
  return text;
}

function main(argv: string[]): number {
  const check = argv[0] === "--check";
  const out = check ? undefined : argv[0];
  const entries = argv.slice(1);
  if (!check && (!out || out.startsWith("-"))) {
    console.error(
      "usage: pipeline-config.ts <out.toml> <section.key=value>... | --check <section.key=value>..."
    );
    return 2;
  }
  try {
    const parsed = entries.map(parseSetting);
    if (out) writePipelineConfig(out, parsed);
    else collectSettings(parsed);
    return 0;
  } catch (e) {
    console.error(`pipeline-config: ${(e as Error).message}`);
    return 2;
  }
}

if (
  process.argv[1] &&
  path.resolve(process.argv[1]) === path.resolve(import.meta.filename)
) {
  process.exit(main(process.argv.slice(2)));
}
