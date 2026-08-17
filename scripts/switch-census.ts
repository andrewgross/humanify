/**
 * Switch census. `npm run census:switches`.
 *
 * Detector #1 in the hunt for legacy/dead/twin code: inventory every toggle
 * the pipeline exposes and classify each by who can set it and what it
 * guards. The motivating find: a ~1,500-line second scheduler reachable only
 * via a flag nobody passed outside tests. A knob nobody can reach, or a knob
 * that selects a maintained twin implementation, is exactly that shape.
 *
 * Three inventories, all over `src/` only:
 *
 * 1. ENV KILL SWITCHES — enumerated from the registry in
 *    `src/kill-switches.ts` (the enforced single reader). Each switch must
 *    have a hand-maintained kind in SWITCH_KIND below:
 *      pass-disable  the non-default branch skips/returns early — fine
 *      twin-selector the flag selects a substantial alternative code path —
 *                    review: is the twin still earning its keep?
 *      probe-enable  the flag turns ON instrumentation; default is inert
 *    A registry switch missing from SWITCH_KIND FAILS the census, so every
 *    future switch declares its kind at review time. A switch with zero
 *    `switchOn(...)` read sites FAILS as ORPHAN.
 *
 * 2. OPTION FIELDS — the fields of the surveyed option interfaces, each
 *    classified from grep-level evidence:
 *      OK        set by some non-test caller AND read somewhere
 *      DEAD-KNOB read but never set outside tests — physically unreachable
 *      VESTIGIAL set but never read — plumbing to nowhere
 *      UNUSED    neither — a declaration and nothing else
 *    "Set" evidence includes a commander `.option("--flag")` registration
 *    whose camelized name is the field: for CLI-parsed option bags the
 *    parser is the setter and no literal `field:` ever appears in src. The
 *    first run misclassified 18 fields DEAD-KNOB by missing exactly this.
 *    Anything not OK must be on FIELD_ALLOWLIST (with a reason) or the
 *    census exits 1 — green now, ratchets on new offenders.
 *
 * 3. CLI FLAGS — every commander `.option(...)` registration in
 *    `src/commands/`, mapped to its option field; a flag whose field is in a
 *    bad class is reported (allowlisted via FIELD_ALLOWLIST on the field).
 *
 * ## What this census CANNOT see (grep-level, by design)
 *
 * Dynamic dispatch, spread objects (`{ ...opts }`), computed keys
 * (`opts[name]`), and cross-command reachability (a field set on one command
 * while the live path never passes it — the `--split-strategy` class, see
 * KNOWN_FLAG_NOTES) are all invisible here. Shorthand `{ field }` is
 * ambiguous between set and read and is counted as BOTH, so it can hide an
 * offender but never invent one. Type-annotation noise can inflate SET
 * counts. The execution-count census is the complementary detector; this one
 * only rules things IN for review, never out.
 *
 * Static analysis only: reads files, greps, exits. Runs nothing.
 */
import * as fs from "node:fs";
import * as path from "node:path";

const REPO = path.resolve(import.meta.dirname, "..");
const SRC = path.join(REPO, "src");

// ---------------------------------------------------------------------------
// Corpus: every non-test .ts under src/, comments stripped.
// ---------------------------------------------------------------------------

interface SourceFile {
  /** Path relative to the repo root, e.g. "src/commands/unified.ts". */
  rel: string;
  /** Comment-stripped text (line structure preserved). */
  text: string;
  lines: string[];
}

function listTsFiles(dir: string): string[] {
  const out: string[] = [];
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) out.push(...listTsFiles(full));
    else if (
      entry.name.endsWith(".ts") &&
      !entry.name.endsWith(".test.ts") &&
      !entry.name.endsWith(".e2etest.ts")
    )
      out.push(full);
  }
  return out;
}

/** Strip a `//` comment from one line, respecting string literals. */
function stripLineComment(line: string): string {
  let quote: string | null = null;
  for (let i = 0; i < line.length; i++) {
    const c = line[i];
    if (quote) {
      if (c === "\\") i++;
      else if (c === quote) quote = null;
    } else if (c === '"' || c === "'" || c === "`") quote = c;
    else if (c === "/" && line[i + 1] === "/") return line.slice(0, i);
  }
  return line;
}

/**
 * Strip comments while preserving line count. Block comments go first via
 * regex (naive: a string containing an unmatched `/*` would confuse it —
 * none does today), then line comments with quote awareness.
 */
function stripComments(text: string): string {
  const noBlocks = text.replace(/\/\*[\s\S]*?\*\//g, (m) =>
    m.replace(/[^\n]/g, " ")
  );
  return noBlocks.split("\n").map(stripLineComment).join("\n");
}

function loadCorpus(): SourceFile[] {
  return listTsFiles(SRC).map((full) => {
    const text = stripComments(fs.readFileSync(full, "utf8"));
    return { rel: path.relative(REPO, full), text, lines: text.split("\n") };
  });
}

// ---------------------------------------------------------------------------
// Part 1: env kill switches.
// ---------------------------------------------------------------------------

type SwitchKind = "pass-disable" | "twin-selector" | "probe-enable";

/**
 * Hand-maintained: the kind of every registry switch, decided at review
 * time by reading the guarded branch. A registry switch absent here fails
 * the census — that is the ratchet.
 */
const SWITCH_KIND: Record<string, { kind: SwitchKind; note: string }> = {
  "family-permute": {
    kind: "pass-disable",
    note: "rename/plugin.ts early-returns before the permute pass"
  },
  "shingle-probe": {
    kind: "probe-enable",
    note: "prior-version.ts shingle census; unset is the normal path"
  },
  "content-anchor": {
    kind: "pass-disable",
    note: "stable-split.ts returns the tier untouched"
  },
  "anchor-preempt": {
    kind: "pass-disable",
    note: "stable-split.ts keeps the name vote above the anchor"
  },
  "anchor-nearident": {
    kind: "pass-disable",
    note: "stable-split.ts drops the near-identical disjunct"
  },
  "allsame-vote": {
    kind: "pass-disable",
    note: "stable-split.ts skips the all-same rescue tier"
  },
  "empty-decl-hash-guard": {
    kind: "pass-disable",
    note: "stable-split.ts drops the refusal guard (hash tier claims freely)"
  },
  "registrar-exemption": {
    kind: "pass-disable",
    note: "split/load-order.ts puts registrars back under the barrier"
  },
  "emit-align": {
    kind: "pass-disable",
    note: "cjs-emit.ts/stable-split.ts keep fresh statement order"
  },
  "name-align": {
    kind: "pass-disable",
    note: "alignment keys fall back to hash alone"
  },
  "vendor-inherit": {
    kind: "pass-disable",
    note: "vendor-body-inherit.ts re-emits vendor bodies fresh"
  },
  "manifest-prior-order": {
    kind: "pass-disable",
    note: "manifest-order.ts keeps fresh manifest order"
  },
  "post-split-reconcile": {
    kind: "pass-disable",
    note: "post-split-reconcile.ts skips the whole pass"
  },
  "fossil-graded-content": {
    kind: "pass-disable",
    note:
      "fossil-match.ts skips the graded tier; enclosures fall back to the " +
      "per-statement equality cliff, which rejects pairs measured at ~86% similar"
  },
  "fossil-graph-position": {
    kind: "pass-disable",
    note:
      "fossil-match.ts skips tier D; an enclosure that rewrote its body keeps " +
      "minting a fresh identity, which is the pre-exp078 behaviour"
  },
  "fossil-split": {
    kind: "pass-disable",
    note:
      "stable-split.ts skips fossil grouping; the assignment it falls to " +
      "is the SAME one every non-bun input and every release-1 run takes, " +
      "not a twin kept alive by this switch (exp070 rollout safety)"
  }
};

function parseKillSwitchRegistry(): string[] {
  const text = fs.readFileSync(path.join(SRC, "kill-switches.ts"), "utf8");
  const start = text.indexOf("export const KILL_SWITCHES");
  const body = text.slice(start, text.indexOf("} as const", start));
  const names = [...body.matchAll(/^ {2}"([a-z][a-z0-9-]+)": \{/gm)].map(
    (m) => m[1]
  );
  // Self-test hook: the unit test injects a fake unclassified switch and
  // asserts the census goes RED — proving the ratchet CAN fire, so its
  // green means something (a detector whose failure path never ran is a
  // zero nobody validated).
  if (process.env.SWITCH_CENSUS_INJECT_FAKE_SWITCH === "1") {
    names.push("fake-switch-for-self-test");
  }
  return names;
}

/** `const ALIAS = "HUMANIFY_..."` bindings, so aliased reads count. */
function findEnvAliases(corpus: SourceFile[]): Map<string, string> {
  const aliases = new Map<string, string>();
  for (const f of corpus) {
    for (const m of f.text.matchAll(
      /const (\w+)\s*=\s*"([a-z][a-z0-9-]+)"(?=;| )/g
    )) {
      aliases.set(m[1], m[2]);
    }
  }
  return aliases;
}

interface SwitchRow {
  name: string;
  kind: string;
  reads: number;
  files: string[];
  note: string;
  problem: string | null;
}

function countSwitchReads(
  corpus: SourceFile[],
  aliases: Map<string, string>
): Map<string, { reads: number; files: Set<string> }> {
  const counts = new Map<string, { reads: number; files: Set<string> }>();
  const readRe = /switchOn\(\s*(?:"([a-z][a-z0-9-]+)"|(\w+))\s*\)/g;
  for (const f of corpus) {
    if (f.rel === "src/kill-switches.ts") continue;
    for (const m of f.text.matchAll(readRe)) {
      const name = m[1] ?? aliases.get(m[2]);
      if (!name) continue;
      const entry = counts.get(name) ?? { reads: 0, files: new Set() };
      entry.reads++;
      entry.files.add(f.rel);
      counts.set(name, entry);
    }
  }
  return counts;
}

function censusKillSwitches(corpus: SourceFile[]): SwitchRow[] {
  const registry = parseKillSwitchRegistry();
  const counts = countSwitchReads(corpus, findEnvAliases(corpus));
  const rows: SwitchRow[] = registry.map((name) => {
    const kind = SWITCH_KIND[name];
    const c = counts.get(name);
    const problem = !kind
      ? "UNCLASSIFIED — add it to SWITCH_KIND with a reviewed kind"
      : !c
        ? "ORPHAN — in the registry but envFlag() never reads it"
        : kind.kind === "twin-selector"
          ? "TWIN-SELECTOR — review whether the twin path still earns its keep"
          : null;
    return {
      name,
      kind: kind?.kind ?? "?",
      reads: c?.reads ?? 0,
      files: [...(c?.files ?? [])],
      note: kind?.note ?? "",
      problem
    };
  });
  for (const name of Object.keys(SWITCH_KIND)) {
    if (!registry.includes(name)) {
      rows.push({
        name,
        kind: SWITCH_KIND[name].kind,
        reads: 0,
        files: [],
        note: SWITCH_KIND[name].note,
        problem: "STALE — in SWITCH_KIND but no longer in the registry"
      });
    }
  }
  return rows;
}

// ---------------------------------------------------------------------------
// Part 2: option fields.
// ---------------------------------------------------------------------------

/** The option interfaces this census surveys. Extends-clauses are NOT
 * followed (SplitOptions extends ClusterOptions; only its own fields show). */
const SURVEYED_INTERFACES: ReadonlyArray<{ file: string; name: string }> = [
  { file: "src/analysis/types.ts", name: "ProcessorOptions" },
  { file: "src/rename/plugin.ts", name: "RenamePluginOptions" },
  { file: "src/commands/settings.ts", name: "SettingsInput" },
  { file: "src/commands/settings.ts", name: "LeverSettings" },
  { file: "src/commands/settings.ts", name: "Settings" },
  { file: "src/commands/unified.ts", name: "CommandOptions" },
  { file: "src/commands/unified.ts", name: "FlagExplicitness" }
];

/**
 * Known offenders, kept green so NEW offenders fail. Key is
 * "Interface.field"; the value says why it is tolerated (or what to do).
 * Empty as of 2026-08-10: the first full run found no unreachable or
 * unread option field at grep level.
 */
const FIELD_ALLOWLIST: Record<string, string> = {};

/**
 * Hand-notes for flags whose problem grep cannot see (cross-command
 * reachability). Informational — printed, never failing.
 */
const KNOWN_FLAG_NOTES: Record<string, string> = {
  "--split-strategy":
    "registered on the standalone `split` command only; the unified path " +
    "never passes it — documented in docs/pipeline-stages.md"
};

interface InterfaceSpec {
  iface: string;
  rel: string;
  startLine: number;
  endLine: number;
  fields: string[];
}

/** Net brace depth change contributed by one line. */
function braceDelta(line: string): number {
  let delta = 0;
  for (const c of line) {
    if (c === "{") delta++;
    else if (c === "}") delta--;
  }
  return delta;
}

/** Locate `interface Name {` / `type Name = {` and brace-match its body.
 * Fields are collected at depth 1 only, so nested object types (e.g.
 * `position: { line: number }`) do not leak their members in. */
function extractInterface(
  file: SourceFile,
  name: string
): InterfaceSpec | null {
  const headRe = new RegExp(`(?:interface|type) ${name}\\b[^{]*\\{`);
  const startIdx = file.lines.findIndex((l) => headRe.test(l));
  if (startIdx < 0) return null;
  const fields: string[] = [];
  const fieldRe = /^\s*(?:readonly\s+)?(\w+)\??:/;
  let depth = 0;
  for (let i = startIdx; i < file.lines.length; i++) {
    const line = file.lines[i];
    const m = depth === 1 ? fieldRe.exec(line) : null;
    if (m) fields.push(m[1]);
    depth += braceDelta(line);
    if (depth <= 0 && i > startIdx) {
      return {
        iface: name,
        rel: file.rel,
        startLine: startIdx,
        endLine: i,
        fields
      };
    }
  }
  return null;
}

/** Corpus with every surveyed interface body blanked out, so declarations
 * never count as evidence. */
function blankSurveyedBodies(
  corpus: SourceFile[],
  specs: InterfaceSpec[]
): SourceFile[] {
  return corpus.map((f) => {
    const ranges = specs.filter((s) => s.rel === f.rel);
    if (ranges.length === 0) return f;
    const lines = [...f.lines];
    for (const r of ranges) {
      for (let i = r.startLine; i <= r.endLine; i++) lines[i] = "";
    }
    return { rel: f.rel, text: lines.join("\n"), lines };
  });
}

interface Evidence {
  set: number;
  read: number;
  ambig: number;
  /** Commander itself sets the field: a `.option("--flag")` registration
   * camelizes to it. The parse populates the options object, so a literal
   * `field:` set never appears in src for CLI-parsed bags. */
  cliSet: boolean;
}

function countMatches(text: string, re: RegExp): number {
  return [...text.matchAll(re)].length;
}

/** Lines that are type-member declarations in some unsurveyed interface —
 * excluded from SET evidence. Optional members carry `?:`; required ones
 * look like `name: SomeType;` — the trailing `;` is required, because this
 * repo's prettier writes no trailing commas, so a last object-literal
 * property `field: value` is otherwise indistinguishable. */
function isDeclarationLine(line: string, field: string): boolean {
  if (new RegExp(`\\b${field}\\?:`).test(line)) return true;
  return new RegExp(
    `^\\s*(?:readonly\\s+)?${field}:\\s*[\\w$.<>|&\\[\\] ]+;\\s*$`
  ).test(line);
}

function fieldEvidence(
  blanked: SourceFile[],
  field: string,
  cliFields: ReadonlySet<string>
): Evidence {
  const setRe = new RegExp(`(?:^\\s*|[{,]\\s*)${field}\\s*:`);
  const dotRe = new RegExp(`\\.${field}\\b`, "g");
  const bareNullishRe = new RegExp(`(?<![.\\w])${field}\\s*\\?\\?`, "g");
  const destructureRe = new RegExp(
    `\\{[^{}]*(?<![.\\w])${field}\\b[^{}]*\\}\\s*[:=]`,
    "g"
  );
  const ambigRe = new RegExp(`[{,]\\s*(?<![.\\w])${field}\\s*[,}]`, "g");
  const e: Evidence = {
    set: 0,
    read: 0,
    ambig: 0,
    cliSet: cliFields.has(field)
  };
  for (const f of blanked) {
    for (const line of f.lines) {
      if (setRe.test(line) && !isDeclarationLine(line, field)) e.set++;
    }
    e.read += countMatches(f.text, dotRe);
    e.read += countMatches(f.text, bareNullishRe);
    e.read += countMatches(f.text, destructureRe);
    e.ambig += countMatches(f.text, ambigRe);
  }
  return e;
}

type FieldClass = "OK" | "DEAD-KNOB" | "VESTIGIAL" | "UNUSED";

function classifyField(e: Evidence): FieldClass {
  const set = e.set + e.ambig > 0 || e.cliSet;
  const read = e.read + e.ambig > 0;
  if (set && read) return "OK";
  if (read) return "DEAD-KNOB";
  if (set) return "VESTIGIAL";
  return "UNUSED";
}

interface FieldRow {
  iface: string;
  field: string;
  cls: FieldClass;
  e: Evidence;
  allowed: string | null;
}

function censusOptionFields(
  corpus: SourceFile[],
  cliFields: ReadonlySet<string>
): {
  rows: FieldRow[];
  missing: string[];
} {
  const specs: InterfaceSpec[] = [];
  const missing: string[] = [];
  for (const s of SURVEYED_INTERFACES) {
    const file = corpus.find((f) => f.rel === s.file);
    const spec = file ? extractInterface(file, s.name) : null;
    if (spec) specs.push(spec);
    else missing.push(`${s.name} (${s.file})`);
  }
  const blanked = blankSurveyedBodies(corpus, specs);
  const rows: FieldRow[] = [];
  for (const spec of specs) {
    for (const field of spec.fields) {
      const e = fieldEvidence(blanked, field, cliFields);
      rows.push({
        iface: spec.iface,
        field,
        cls: classifyField(e),
        e,
        allowed: FIELD_ALLOWLIST[`${spec.iface}.${field}`] ?? null
      });
    }
  }
  return { rows, missing };
}

// ---------------------------------------------------------------------------
// Part 3: CLI flags.
// ---------------------------------------------------------------------------

interface FlagRow {
  flag: string;
  field: string;
  rel: string;
  cls: FieldClass | "unsurveyed";
  note: string;
}

function camelize(flag: string): string {
  const base = flag.startsWith("no-") ? flag.slice(3) : flag;
  return base.replace(/-([a-z0-9])/g, (_, c: string) => c.toUpperCase());
}

function extractCliFlags(
  corpus: SourceFile[]
): Array<{ flag: string; rel: string }> {
  const out: Array<{ flag: string; rel: string }> = [];
  const seen = new Set<string>();
  for (const f of corpus) {
    if (!f.rel.startsWith("src/commands/")) continue;
    for (const m of f.text.matchAll(/\.option\(\s*"([^"]+)"/g)) {
      for (const flag of m[1].matchAll(/--[a-z][a-z0-9-]*/g)) {
        const key = `${f.rel}:${flag[0]}`;
        if (seen.has(key)) continue;
        seen.add(key);
        out.push({ flag: flag[0], rel: f.rel });
      }
    }
  }
  return out;
}

function censusCliFlags(
  corpus: SourceFile[],
  fieldRows: FieldRow[]
): FlagRow[] {
  return extractCliFlags(corpus).map(({ flag, rel }) => {
    const field = camelize(flag.slice(2));
    const matches = fieldRows.filter((r) => r.field === field);
    const worst =
      matches.find((r) => r.cls !== "OK" && !r.allowed) ??
      matches.find((r) => r.cls !== "OK") ??
      matches[0];
    return {
      flag,
      field,
      rel,
      cls: worst ? worst.cls : "unsurveyed",
      note:
        KNOWN_FLAG_NOTES[flag] ??
        (worst?.allowed ? `allowlisted: ${worst.allowed}` : "")
    };
  });
}

// ---------------------------------------------------------------------------
// Output.
// ---------------------------------------------------------------------------

function printTable(title: string, header: string[], rows: string[][]): void {
  console.log(`\n━━━ ${title} ━━━`);
  const widths = header.map((h, i) =>
    Math.max(h.length, ...rows.map((r) => r[i].length))
  );
  const fmt = (r: string[]): string =>
    r.map((c, i) => c.padEnd(widths[i])).join("  ");
  console.log(fmt(header));
  console.log(widths.map((w) => "─".repeat(w)).join("  "));
  for (const r of rows) console.log(fmt(r));
}

function printSwitchTable(rows: SwitchRow[]): void {
  printTable(
    "env kill switches (registry: src/kill-switches.ts)",
    ["switch", "kind", "reads", "note"],
    rows.map((r) => [
      r.name,
      r.problem ?? r.kind,
      `${r.reads} in ${r.files.length} file(s)`,
      r.note
    ])
  );
}

function printFieldTable(rows: FieldRow[]): void {
  printTable(
    "option fields (set/read/ambiguous-shorthand occurrences in non-test src/)",
    ["interface.field", "class", "set", "read", "ambig", "cli", "note"],
    rows.map((r) => [
      `${r.iface}.${r.field}`,
      r.cls,
      `${r.e.set}`,
      `${r.e.read}`,
      `${r.e.ambig}`,
      r.e.cliSet ? "yes" : "",
      r.allowed ? `allowlisted: ${r.allowed}` : ""
    ])
  );
}

function printFlagTable(rows: FlagRow[]): void {
  printTable(
    "CLI flags (src/commands/ .option registrations)",
    ["flag", "field", "class", "note"],
    rows.map((r) => [r.flag, r.field, r.cls, r.note])
  );
}

function summarize(
  switches: SwitchRow[],
  fields: FieldRow[],
  flags: FlagRow[],
  missing: string[]
): number {
  const switchProblems = switches.filter((s) => s.problem);
  const badFields = fields.filter((f) => f.cls !== "OK");
  const offenders = badFields.filter((f) => !f.allowed);
  const flagOffenders = flags.filter(
    (f) => f.cls !== "OK" && f.cls !== "unsurveyed" && !f.note
  );
  const counts = (cls: FieldClass): number =>
    fields.filter((f) => f.cls === cls).length;
  console.log(`\n${"═".repeat(72)}`);
  console.log(
    `SUMMARY: ${switches.length} switches (${switchProblems.length} problems) | ` +
      `${fields.length} fields: ${counts("OK")} OK, ` +
      `${counts("DEAD-KNOB")} DEAD-KNOB, ${counts("VESTIGIAL")} VESTIGIAL, ` +
      `${counts("UNUSED")} UNUSED (${badFields.length - offenders.length} allowlisted) | ` +
      `${flags.length} flags`
  );
  for (const s of switchProblems)
    console.log(`PROBLEM: ${s.name} — ${s.problem}`);
  for (const f of offenders)
    console.log(
      `OFFENDER: ${f.iface}.${f.field} is ${f.cls} — fix it or allowlist with a reason`
    );
  for (const f of flagOffenders)
    console.log(`OFFENDER: ${f.flag} maps to a ${f.cls} field ${f.field}`);
  for (const m of missing)
    console.log(`PROBLEM: surveyed interface not found: ${m}`);
  const failed =
    switchProblems.length +
    offenders.length +
    flagOffenders.length +
    missing.length;
  console.log(
    failed === 0 ? "CENSUS GREEN" : `CENSUS RED (${failed} finding(s))`
  );
  return failed === 0 ? 0 : 1;
}

function main(): number {
  if (process.argv.includes("--help")) {
    console.log(
      "switch-census: inventory env kill switches, option fields and CLI " +
        "flags under src/, classifying each toggle by who can set it and " +
        "what it guards. See the header comment for the classes and for " +
        "what a grep-level census cannot see. Exit 0 = green."
    );
    return 0;
  }
  const corpus = loadCorpus();
  const switches = censusKillSwitches(corpus);
  const cliFields = new Set(
    extractCliFlags(corpus).map(({ flag }) => camelize(flag.slice(2)))
  );
  const { rows: fields, missing } = censusOptionFields(corpus, cliFields);
  const flags = censusCliFlags(corpus, fields);
  printSwitchTable(switches);
  printFieldTable(fields);
  printFlagTable(flags);
  return summarize(switches, fields, flags, missing);
}

process.exit(main());
