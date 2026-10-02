/**
 * exp094 census, part 1 — the STATEMENT-level wrapper-spelling flip population
 * over the frozen walk trees (/work/walk-rust-0926), on the SAME composition
 * the eval scores (composeDiff, the 034 harness's layout instrument).
 *
 * Per hop (adjacent walked versions), for `src` and `vendor` separately:
 *
 *   - the full composeDiff tally (so the hop's `real` and the advisory
 *     `spellingIdenticalLines` can be read next to each other, and the flip
 *     share of the charged mass can be stated exactly);
 *   - every spelling sample (kind "spelling") with its file, charged lines and
 *     both texts' wrapper forms (derived by re-matching the heads — the sample
 *     carries the raw statements, so no classification is trusted on faith);
 *   - a "post-repair" read: what `real` WOULD read if backlog item 12's
 *     head-tolerant pairing landed (real - spellingIdenticalLines). REPORT
 *     ONLY — the charge itself stands per Andrew's 2026-09-29 decision, and
 *     this script changes no living instrument.
 *
 * Incremental: writes one JSON per hop into out/ and skips hops already on
 * disk, so the sweep can run unattended and be resumed.
 *
 * Usage:
 *   npx tsx census.ts <trees-dir> <out-dir> [--surface src|vendor|both]
 */
import * as fs from "node:fs";
import * as path from "node:path";
import {
  composeDiff,
  type NoiseSample
} from "../037-noise-source-decomposition/diff-composition.js";

interface Flags {
  treesDir: string;
  outDir: string;
  surfaces: Array<"src" | "vendor">;
}

function parseFlags(argv: string[]): Flags {
  const flags: Flags = {
    treesDir: "",
    outDir: "",
    surfaces: ["src", "vendor"]
  };
  for (let i = 0; i < argv.length; i += 1) {
    const a = argv[i];
    if (a === "--surface") {
      const v = argv[++i];
      if (v !== "src" && v !== "vendor" && v !== "both") {
        throw new Error(`--surface must be src|vendor|both, got ${v}`);
      }
      flags.surfaces = v === "both" ? ["src", "vendor"] : [v];
    } else if (!flags.treesDir) {
      flags.treesDir = resolve(a);
    } else if (!flags.outDir) {
      flags.outDir = resolve(a);
    } else {
      throw new Error(`unknown argument: ${a}`);
    }
  }
  if (!flags.treesDir || !flags.outDir) {
    throw new Error("usage: census.ts <trees-dir> <out-dir> [--surface ...]");
  }
  return flags;
}

function resolve(p: string): string {
  return path.resolve(p);
}

/** The wrapper form of one spelling sample's statement text, by re-matching
 * the tested head rule — never trusted from the detector's own decision. */
function headForm(text: string): {
  form: "arrow" | "function";
  head: string;
} | null {
  const firstLine = text.split("\n", 1)[0];
  const call = /^([^;{]*\)\()/.exec(firstLine);
  if (!call) return null;
  const afterCall = firstLine.slice(call[0].length);
  const heads: Array<{ re: RegExp; form: "arrow" | "function" }> = [
    { re: /^function\s*\(([^()]*)\)\s*\{/, form: "function" },
    { re: /^\(([^()]*)\)\s*=>\s*\{/, form: "arrow" },
    { re: /^([A-Za-z_$][\w$]*)\s*=>\s*\{/, form: "arrow" }
  ];
  for (const h of heads) {
    const m = h.re.exec(afterCall);
    if (m) return { form: h.form, head: m[0] };
    // the sampled statements end with the call head followed by the wrapper,
    // but a sample text is the FULL statement: the head match above is on the
    // first line after the callee, which is where the wrapper starts.
  }
  return null;
}

/** The callee the sequence-call head invokes — the wrapper's RECEIVER, whose
 * own code is what could observe the wrapper value (`.prototype`, `new`). */
function receiverOf(text: string): string | null {
  const firstLine = text.split("\n", 1)[0];
  const m = /\(\s*0\s*,\s*([\w$.]+)\s*\)\(/.exec(firstLine);
  return m ? m[1] : null;
}

function hopVersions(treesDir: string): string[][] {
  const versions = fs
    .readdirSync(treesDir)
    .filter((v) => v.startsWith("2.1."))
    .sort();
  const hops: string[][] = [];
  for (let i = 1; i < versions.length; i += 1) {
    hops.push([versions[i - 1], versions[i]]);
  }
  return hops;
}

function main(): void {
  const flags = parseFlags(process.argv.slice(2));
  fs.mkdirSync(flags.outDir, { recursive: true });
  console.log(`census over ${flags.treesDir} -> ${flags.outDir}`);
  for (const [from, to] of hopVersions(flags.treesDir)) {
    const outFile = path.join(flags.outDir, `${from}--${to}.json`);
    let existing: Record<string, unknown> | null = null;
    if (fs.existsSync(outFile)) {
      existing = JSON.parse(fs.readFileSync(outFile, "utf8"));
      const surfaces = (existing?.surfaces ?? {}) as Record<string, unknown>;
      if (flags.surfaces.every((s) => surfaces[s] !== undefined)) {
        console.log(`  ${from}->${to}: cached`);
        continue;
      }
    }
    const record: {
      hop: string;
      surfaces: Record<string, unknown>;
    } = {
      hop: `${from}->${to}`,
      surfaces:
        (existing?.surfaces as Record<string, unknown> | undefined) ?? {}
    };
    for (const surface of flags.surfaces) {
      const priorDir = path.join(flags.treesDir, from, surface);
      const freshDir = path.join(flags.treesDir, to, surface);
      if (!fs.existsSync(priorDir) || !fs.existsSync(freshDir)) {
        console.log(`  ${from}->${to} ${surface}: missing, skipped`);
        continue;
      }
      const samples: NoiseSample[] = [];
      const t = composeDiff(priorDir, freshDir, { samples, cap: 100_000 });
      const spelling = samples.filter((s) => s.kind === "spelling");
      const entry = {
        real: t.real,
        naming: t.naming,
        alias: t.alias,
        reorder: t.reorder,
        fileAddRemove: t.fileAddRemove,
        spellingIdenticalLines: t.spellingIdenticalLines,
        postRepairReal: t.real - t.spellingIdenticalLines,
        flips: spelling.length,
        samples: spelling.map((s) => ({
          file: s.file,
          lines: s.lines,
          priorForm: headForm(s.priorText ?? "")?.form ?? null,
          freshForm: headForm(s.freshText ?? "")?.form ?? null,
          receiver: receiverOf(s.freshText ?? s.priorText ?? ""),
          priorLines: (s.priorText ?? "").split("\n").length,
          freshLines: (s.freshText ?? "").split("\n").length
        }))
      };
      record.surfaces[surface] = entry;
      console.log(
        `  ${from}->${to} ${surface}: real ${t.real}, spelling ${t.spellingIdenticalLines} (${spelling.length} flips)`
      );
    }
    fs.writeFileSync(outFile, `${JSON.stringify(record, null, 2)}\n`);
  }
}

main();
