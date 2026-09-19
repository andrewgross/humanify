/**
 * The dump writer: assembles the artifact catalog (07 §2) from the recorders
 * and the run's results, and writes it to the --dump-artifacts directory.
 *
 * Called at the boundary unified.ts already has (after the split and the
 * stats/diagnostics writes) — never a second pipeline pass. Files:
 *
 *   meta.json        commit, flags, input paths, text sha256 anchors
 *   text/fresh.js    the beautified fresh text (the shared input)
 *   text/prior.js    the prior carry bundle, when a --prior-version was given
 *   text/minified.js the run's original minified input (regions' anchor)
 *   text/shipped.js  the split's input text (split-era spans' anchor)
 *   functions.json   the graph's pre-naming state (capture.ts)
 *   partitions.json  the three hash families, member -> opaque hash
 *   matches.json     cascade pairs + rejections + ResolutionStats
 *   transfers.json   every applied and rejected rename with tier + reason
 *   votes.json       vote tallies with witnesses + ladder outcome
 *   prompts.jsonl    every rendered prompt, dispatch order, cache keys
 *   names.json       span-keyed final assignment per binding
 *   placement.json   file + tier + evidence per statement span
 *   emit.json        emitted layout: statement spans per file slot + aliases
 *   tree-manifest.json  per-file sha256 of the emitted tree
 *   regions.json     library comment regions + Bun banner classifications
 *
 * All row arrays are sorted by their (text, start, end) key before writing;
 * all map-shaped data is written as sorted pairs. The UTF-16 -> UTF-8 byte
 * conversion happens HERE, once per anchored text: recorders capture RAW
 * UTF-16 spans, and the ByteOffsetTable (built in one O(n) pass per text,
 * with the identity fast path) converts every endpoint exactly once.
 */
import { createHash } from "node:crypto";
import * as fs from "node:fs";
import * as path from "node:path";
import { execFileSync } from "node:child_process";
import { artifactDump, type DumpFunctionRow } from "./artifacts.js";
import { ByteOffsetTable } from "./spans.js";
import {
  DUMP_SCHEMA_VERSION,
  spanKeyOrder,
  type SpanKey
} from "./serialize.js";
import { sha256Hex } from "../rename/rename-ledger.js";
import { strategyTrail } from "../rename/strategy-trail.js";
import { placementTrail } from "../split/placement-trail.js";
import {
  bunManifestPath,
  type BunModulesManifest
} from "../unpack/adapters/bun.js";

/** Every anchored-text label (07 §1, WP0.2's multi-text amendment). */
export type DumpAnchorLabel =
  | "fresh"
  | "generated"
  | "reconciled"
  | "prior"
  | "minified"
  | "shipped";

/** The anchored texts and their converters — keyed by LABEL. */
class Anchors {
  fresh?: string;
  generated?: string;
  reconciled?: string;
  prior?: string;
  minified?: string;
  shipped?: string;
  private tables = new Map<string, ByteOffsetTable>();

  set(label: DumpAnchorLabel, content: string | undefined): void {
    if (label === "fresh") this.fresh = content;
    else if (label === "prior") this.prior = content;
    else if (label === "shipped") this.shipped = content;
    else if (label === "generated") this.generated = content;
    else if (label === "reconciled") this.reconciled = content;
    else this.minified = content;
  }

  private table(label: DumpAnchorLabel): ByteOffsetTable | undefined {
    const content =
      label === "fresh"
        ? this.fresh
        : label === "prior"
          ? this.prior
          : label === "shipped"
            ? this.shipped
            : label === "generated"
              ? this.generated
              : label === "reconciled"
                ? this.reconciled
                : this.minified;
    if (content === undefined) return undefined;
    let table = this.tables.get(label);
    if (!table) {
      table = ByteOffsetTable.for(content);
      this.tables.set(label, table);
    }
    return table;
  }

  /** Convert a raw UTF-16 span pair against one anchored text. */
  convert(
    label: DumpAnchorLabel,
    raw: { start: number; end: number }
  ): SpanKey {
    if (raw.start < 0) {
      // The no-position sentinel (synthesized identifiers) — pass through.
      return { text: label, start: raw.start, end: raw.end };
    }
    const table = this.table(label);
    if (!table) return { text: label, start: raw.start, end: raw.end };
    try {
      return {
        text: label,
        start: table.toByte(raw.start),
        end: table.toByte(raw.end)
      };
    } catch (err) {
      // Name the row in the failure: a span that outlives its anchor is a
      // recorder bug, and the row is the only way back to the site.
      throw new Error(
        `dump: span ${label}[${raw.start}..${raw.end}) rejected: ` +
          `${err instanceof Error ? err.message : String(err)}`
      );
    }
  }
}

/** One section writer's shared context. */
interface Writer {
  dir: string;
  anchors: Anchors;
  convertKey: (label: DumpAnchorLabel, key: SpanKey | null) => SpanKey | null;
}

export interface DumpWriteArgs {
  dir: string;
  /** The run's resolved selection + flags that affect decisions. */
  flags: Record<string, unknown>;
  /** The output tree root, for the tree manifest. */
  outputDir: string;
}

function gitShortSha(): string {
  try {
    return execFileSync("git", ["rev-parse", "--short", "HEAD"], {
      encoding: "utf8"
    }).trim();
  } catch {
    return "unknown";
  }
}

/** sha256 of every file in the emitted tree (tree-manifest.json). */
function treeManifest(outputDir: string): {
  files: Array<{ path: string; sha256: string; bytes: number }>;
} {
  const files: Array<{ path: string; sha256: string; bytes: number }> = [];
  const walk = (dir: string, rel: string): void => {
    for (const entry of fs
      .readdirSync(dir, { withFileTypes: true })
      .sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0))) {
      const childRel = rel ? `${rel}/${entry.name}` : entry.name;
      const child = path.join(dir, entry.name);
      if (entry.isDirectory()) {
        walk(child, childRel);
      } else {
        const buf = fs.readFileSync(child);
        files.push({
          path: childRel,
          sha256: createHash("sha256").update(buf).digest("hex"),
          bytes: buf.length
        });
      }
    }
  };
  walk(outputDir, "");
  return { files };
}

function writeJson(file: string, value: unknown): void {
  fs.writeFileSync(file, JSON.stringify(value));
}

export function writeDumpArtifacts(args: DumpWriteArgs): void {
  const { dir } = args;
  fs.mkdirSync(path.join(dir, "text"), { recursive: true });
  const dump = artifactDump;
  const anchors = new Anchors();
  anchors.set("fresh", dump.texts.fresh);
  anchors.set("prior", dump.texts.prior);
  anchors.set("minified", dump.texts.minified);
  anchors.set("shipped", dump.texts.shipped);
  const writer: Writer = {
    dir,
    anchors,
    convertKey: (label, key) => (key ? anchors.convert(label, key) : null)
  };

  writeMeta(writer, args, dump.texts);
  writeTexts(dump.texts, dir);
  writeFunctions(dump.functions, writer);
  writePartitions(dump, anchors, args, dir);
  writeMatches(dump, anchors, writer);
  writeTransfers(writer);
  writeVotes(dump, anchors, writer);
  writePrompts(dump.prompts, anchors, dir);
  writeNames(dump.names, writer);
  writePlacement(writer);
  writeEmit(dump.emitFiles, anchors, dir);
  writeJson(path.join(dir, "tree-manifest.json"), treeManifest(args.outputDir));
  writeRegions(dump, anchors, dir);
}

function writeMeta(
  writer: Writer,
  args: DumpWriteArgs,
  texts: {
    fresh?: string;
    prior?: string;
    minified?: string;
    shipped?: string;
    generated?: string;
    reconciled?: string;
  }
): void {
  const meta = {
    schemaVersion: DUMP_SCHEMA_VERSION,
    generatedAt: new Date().toISOString(),
    commit: gitShortSha(),
    flags: args.flags,
    texts: {
      fresh: texts.fresh ? sha256Hex(texts.fresh) : null,
      prior: texts.prior ? sha256Hex(texts.prior) : null,
      minified: texts.minified ? sha256Hex(texts.minified) : null,
      shipped: texts.shipped ? sha256Hex(texts.shipped) : null,
      generated: texts.generated ? sha256Hex(texts.generated) : null,
      reconciled: texts.reconciled ? sha256Hex(texts.reconciled) : null
    }
  };
  writeJson(path.join(writer.dir, "meta.json"), meta);
}

function writeTexts(
  texts: {
    fresh?: string;
    prior?: string;
    minified?: string;
    shipped?: string;
    generated?: string;
    reconciled?: string;
  },
  dir: string
): void {
  for (const [name, content] of [
    ["fresh", texts.fresh],
    ["prior", texts.prior],
    ["minified", texts.minified],
    ["shipped", texts.shipped],
    ["generated", texts.generated],
    ["reconciled", texts.reconciled]
  ] as const) {
    if (content !== undefined) {
      fs.writeFileSync(path.join(dir, "text", `${name}.js`), content);
    }
  }
}

function writeFunctions(rows: DumpFunctionRow[], writer: Writer): void {
  writeJson(path.join(writer.dir, "functions.json"), {
    schemaVersion: DUMP_SCHEMA_VERSION,
    functions: rows
      .map((row) => ({
        ...row,
        key: writer.anchors.convert("fresh", row.key),
        nameBinding: writer.convertKey("fresh", row.nameBinding),
        internalCallees: row.internalCallees.map((k) =>
          writer.anchors.convert("fresh", k)
        ),
        scopeParent: writer.convertKey("fresh", row.scopeParent),
        bindings: row.bindings.map((b) => ({
          ...b,
          span: writer.anchors.convert("fresh", b.span)
        }))
      }))
      .sort((a, b) => spanKeyOrder(a.key, b.key))
  });
}

/** Partitions: the structuralHash family anchors to fresh, the
 *  statementHash family to shipped (07 §1's four-text amendment); the
 *  structuralSignature family (vendor factories) is read from the written
 *  vendor manifest at this boundary — it keys into the minified text. */
function writePartitions(
  dump: { partitions: import("./artifacts.js").DumpPartitionFamily[] },
  anchors: Anchors,
  args: DumpWriteArgs,
  dir: string
): void {
  const vendorFamily = readVendorSignatureFamily(args.outputDir);
  const families = [
    ...dump.partitions,
    ...(vendorFamily ? [vendorFamily] : [])
  ];
  writeJson(path.join(dir, "partitions.json"), {
    schemaVersion: DUMP_SCHEMA_VERSION,
    families: families.map((family) => ({
      ...family,
      members: family.members
        .map((m) => ({
          member: anchors.convert(familyAnchor(family.family), m.member),
          hash: m.hash
        }))
        .sort((a, b) => spanKeyOrder(a.member, b.member))
    }))
  });
}

function familyAnchor(family: string): DumpAnchorLabel {
  if (family === "statementHash") return "shipped";
  return "fresh";
}

/** The vendor factories' structuralSignature family, read from the
 *  manifest the unpack adapter wrote into vendor/ (adapters/bun.ts). */
function readVendorSignatureFamily(
  outputDir: string
): import("./artifacts.js").DumpPartitionFamily | null {
  const manifestPath = bunManifestPath(outputDir);
  if (!fs.existsSync(manifestPath)) return null;
  let manifest: BunModulesManifest;
  try {
    manifest = JSON.parse(fs.readFileSync(manifestPath, "utf-8"));
  } catch {
    return null;
  }
  const members = manifest.factories
    .filter((f) => f.structuralHash)
    .map((f) => ({
      // The written manifest deliberately carries no factory byte span
      // (exp046 removed it as churn); the member identity is the emitted
      // vendor FILE PATH itself, keyed with a zero span (07 §1's
      // tree-relative-path key space).
      member: { text: f.fileName, start: 0, end: 0 },
      hash: f.structuralHash
    }))
    .sort((a, b) => spanKeyOrder(a.member, b.member));
  if (members.length === 0) return null;
  return { family: "structuralSignature", members };
}

function writeMatches(
  dump: {
    matchPairs: import("./artifacts.js").DumpMatchPair[];
    matchRejections: import("./artifacts.js").DumpMatchRejection[];
  },
  anchors: Anchors,
  writer: Writer
): void {
  writeJson(path.join(writer.dir, "matches.json"), {
    schemaVersion: DUMP_SCHEMA_VERSION,
    pairs: [...dump.matchPairs]
      .map((p) => ({
        ...p,
        prior: anchors.convert("prior", p.prior),
        fresh: anchors.convert("fresh", p.fresh)
      }))
      .sort(
        (a, b) =>
          spanKeyOrder(a.prior, b.prior) || spanKeyOrder(a.fresh, b.fresh)
      ),
    rejections: [...dump.matchRejections]
      .map((r) => ({
        ...r,
        prior: anchors.convert("prior", r.prior),
        candidates: r.candidates
          ?.map((c) => anchors.convert("fresh", c))
          .sort(spanKeyOrder)
      }))
      .sort((a, b) => spanKeyOrder(a.prior, b.prior))
  });
}

/** The strategy trail, span-extended — every applied and rejected rename
 *  with tier + reason, sorted by span. */
function writeTransfers(writer: Writer): void {
  const trailReport = strategyTrail.report();
  writeJson(path.join(writer.dir, "transfers.json"), {
    schemaVersion: DUMP_SCHEMA_VERSION,
    transfers: trailReport.trails
      .map((entry) => ({
        target: (() => {
          try {
            return writer.anchors.convert(
              entry.declText ?? "fresh",
              entry.declSpan ?? { start: -1, end: -1 }
            );
          } catch (err) {
            // Name the row: the tier list identifies the record site.
            throw new Error(
              `transfers row ${entry.oldName} @${entry.loc} ` +
                `(${entry.trail.map((a) => `${a.strategy}:${a.outcome}`).join(",") || "empty"}): ` +
                `${err instanceof Error ? err.message : String(err)}`
            );
          }
        })(),
        oldName: entry.oldName,
        finalName: entry.finalName ?? null,
        settledBy: entry.settledBy,
        attempts: entry.trail.map((a) => ({
          tier: a.strategy,
          outcome: a.outcome,
          reason: a.reason,
          proposedName: a.newName
        }))
      }))
      .sort((a, b) => spanKeyOrder(a.target, b.target))
  });
}

/** The vote-ladder tiers whose trail entry carries a target's outcome. */
const VOTE_LADDER_TIERS = new Set([
  "module-vote",
  "module-pin",
  "fn-name-vote",
  "fn-name-pin",
  "vote-suggest"
]);

/** The ladder outcome per target span, joined from the strategy trail (the
 *  votes recorder snapshots tallies BEFORE the ladders run; the trail's
 *  vote-ladder attempts carry what each ladder did). */
function voteOutcomeBySpan(): Map<string, string> {
  const bySpan = new Map<string, string>();
  for (const row of strategyTrail.report().trails) {
    // Vote rows anchor "fresh" (the votes target naming-era bindings);
    // only rows in that coordinate space join.
    if (!row.declSpan || (row.declText ?? "fresh") !== "fresh") continue;
    const ladder = row.trail.filter((a) => VOTE_LADDER_TIERS.has(a.strategy));
    const last = ladder[ladder.length - 1];
    if (last) {
      bySpan.set(
        `fresh:${row.declSpan.start}:${row.declSpan.end}`,
        last.outcome === "abstained"
          ? `${last.outcome}:${last.reason ?? ""}`
          : last.outcome
      );
    }
  }
  return bySpan;
}

function writeVotes(
  dump: { votes: import("./artifacts.js").DumpVote[] },
  anchors: Anchors,
  writer: Writer
): void {
  const outcomes = voteOutcomeBySpan();
  writeJson(path.join(writer.dir, "votes.json"), {
    schemaVersion: DUMP_SCHEMA_VERSION,
    votes: [...dump.votes]
      .map((v) => ({
        ...v,
        target: anchors.convert("fresh", v.target),
        outcome: outcomes.get(
          `fresh:${(v.target as { start: number }).start}:${(v.target as { end: number }).end}`
        ),
        tally: [...v.tally].sort((a, b) => (a.name < b.name ? -1 : 1)),
        witnesses: v.witnesses.map((w) => ({ ...w }))
      }))
      .sort((a, b) => spanKeyOrder(a.target, b.target))
  });
}

function writePrompts(
  prompts: import("./artifacts.js").DumpPromptRecord[],
  anchors: Anchors,
  dir: string
): void {
  const lines = prompts.map((p) =>
    JSON.stringify({
      ...p,
      targets: p.targets.map((t) => ({
        ...t,
        ...anchors.convert(p.targetsText ?? "fresh", t)
      }))
    })
  );
  fs.writeFileSync(path.join(dir, "prompts.jsonl"), `${lines.join("\n")}\n`);
}

/** names.json: the strategy-trail rows (every tier that settled a binding,
 *  span-keyed by declSpan) MERGED with the recorded rows from the paths the
 *  trail does not cover (uniquify, identity, library prefix) — the recorded
 *  row wins on a span collision. */
function writeNames(
  names: import("./artifacts.js").DumpNameRecord[],
  writer: Writer
): void {
  const trailRows: import("./artifacts.js").DumpNameRecord[] = strategyTrail
    .report()
    .trails.flatMap((entry) => {
      const span = entry.declSpan;
      if (!span || !entry.finalName) return [];
      return [
        {
          target: {
            text: entry.declText ?? "fresh",
            start: span.start,
            end: span.end
          },
          oldName: entry.oldName,
          newName: entry.finalName,
          kind: "function" as const,
          classified: "renamed" as const,
          functionId: `${entry.loc} (${entry.terminalBy ?? entry.settledBy ?? "?"})`
        }
      ];
    });
  const bySpan = new Map<string, import("./artifacts.js").DumpNameRecord>();
  for (const row of [...trailRows, ...names]) {
    bySpan.set(`${row.target.text}:${row.target.start}:${row.target.end}`, row);
  }
  writeJson(path.join(writer.dir, "names.json"), {
    schemaVersion: DUMP_SCHEMA_VERSION,
    names: [...bySpan.values()]
      .map((n) => ({
        ...n,
        target: writer.anchors.convert(
          (n.target.text as DumpAnchorLabel) ?? "fresh",
          n.target
        )
      }))
      .sort((a, b) => spanKeyOrder(a.target, b.target))
  });
}

function writePlacement(writer: Writer): void {
  const trailPlacements = placementTrail.report();
  writeJson(path.join(writer.dir, "placement.json"), {
    schemaVersion: DUMP_SCHEMA_VERSION,
    placements: trailPlacements.trails
      .map((entry) => ({
        key: writer.anchors.convert(
          "shipped",
          entry.span ?? { start: -1, end: -1 }
        ),
        index: entry.index,
        names: entry.names,
        nameCount: entry.nameCount,
        placedBy: entry.placedBy,
        file: entry.file,
        priorFile: entry.priorFile,
        priorFileFrom: entry.priorFileFrom,
        hashMiss: entry.hashMiss,
        alternatives: entry.alternatives,
        evidence: entry.evidence
      }))
      .sort((a, b) => spanKeyOrder(a.key, b.key))
  });
}

function writeEmit(
  emitFiles: import("./artifacts.js").DumpEmitFile[],
  anchors: Anchors,
  dir: string
): void {
  writeJson(path.join(dir, "emit.json"), {
    schemaVersion: DUMP_SCHEMA_VERSION,
    files: [...emitFiles]
      .map((f) => ({
        ...f,
        statements: f.statements.map((s) => ({
          ...s,
          span: anchors.convert("shipped", s.span)
        }))
      }))
      .sort((a, b) => (a.path < b.path ? -1 : a.path > b.path ? 1 : 0))
  });
}

function writeRegions(
  dump: {
    commentRegions: import("./artifacts.js").DumpCommentRegion[];
    bannerClassifications: import("./artifacts.js").DumpBannerClassification[];
  },
  anchors: Anchors,
  dir: string
): void {
  const minifiedSpan = (raw: { start: number; end: number }) => {
    const converted = anchors.convert("minified", raw);
    return { start: converted.start, end: converted.end };
  };
  writeJson(path.join(dir, "regions.json"), {
    schemaVersion: DUMP_SCHEMA_VERSION,
    commentRegions: dump.commentRegions
      .map((r) => ({ ...r, span: minifiedSpan(r.span) }))
      .sort((a, b) => a.span.start - b.span.start),
    bannerClassifications: dump.bannerClassifications
      .map((b) => ({ ...b, span: minifiedSpan(b.span) }))
      .sort((a, b) => a.span.start - b.span.start)
  });
}
