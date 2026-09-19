/**
 * Capture hooks for the artifact dump: the graph walk (functions.json's
 * source + the structuralHash partition family) and the library-region /
 * Bun-classification capture (regions.json). Pure observation — called at
 * points the run already passes through, retaining flat scalar data only.
 */
import type {
  FunctionNode,
  MatchResult,
  ModuleBindingNode,
  UnifiedGraph
} from "../analysis/types.js";
import type { FingerprintIndex } from "../analysis/types.js";
import type { CommentRegion } from "../library-detection/comment-regions.js";
import type { BunModuleClassification } from "../analysis/bun-module-classification.js";
import {
  artifactDump,
  type DumpFunctionRow,
  type DumpPartitionFamily
} from "./artifacts.js";
import type { SpanKey } from "./serialize.js";

/** Raw UTF-16 span of a Babel node, or null without position. */
function rawSpan(
  node: { start?: number | null; end?: number | null } | null | undefined
): {
  start: number;
  end: number;
} | null {
  if (!node || node.start == null || node.end == null) return null;
  return { start: node.start, end: node.end };
}

function freshKey(start: number, end: number): SpanKey {
  // Raw UTF-16 indices; converted to UTF-8 bytes once at write time.
  return { text: "fresh", start, end };
}

/** The fn's own name-binding identifier, when it has one. */
function fnNameIdentifier(fn: FunctionNode): {
  name: string;
  start?: number | null;
  end?: number | null;
} | null {
  const node = fn.path.node;
  const id = "id" in node ? node.id : null;
  if (!id || !("name" in id)) return null;
  return { name: id.name, start: id.start, end: id.end };
}

/**
 * Capture the unified graph's PRE-NAMING state — the exact input the
 * matching cascade consumed: spans, callee edges, scope parents, structural
 * hashes, and per-fn binding slots with ORIGINAL (minified) names. Final
 * shipped names live in the dump's name table, not here. Called once, right
 * after prior-version matching, while the graph is live.
 */
export function captureGraphDump(graph: UnifiedGraph): void {
  if (!artifactDump.isEnabled()) return;
  const rows: DumpFunctionRow[] = [];
  const hashFamily: DumpPartitionFamily = {
    family: "structuralHash",
    members: []
  };
  for (const [, renameNode] of graph.nodes) {
    if (renameNode.type === "function") {
      const fn = renameNode.node as FunctionNode;
      const row = functionRow(fn);
      rows.push(row);
      hashFamily.members.push({
        member: row.key,
        hash: fn.fingerprint.structuralHash
      });
    } else {
      const mb = renameNode.node as ModuleBindingNode;
      const row = moduleBindingRow(mb);
      rows.push(row);
      if (mb.fingerprint) {
        hashFamily.members.push({
          member: row.key,
          hash: mb.fingerprint.structuralHash
        });
      }
    }
  }
  artifactDump.functions = rows;
  artifactDump.partitions = [hashFamily];
}

/** SpanKey-or-sentinel for a raw span: -1/-1 marks no position. */
function keyOf(raw: { start: number; end: number } | null): SpanKey {
  return raw
    ? freshKey(raw.start, raw.end)
    : { text: "fresh", start: -1, end: -1 };
}

function functionRow(fn: FunctionNode): DumpFunctionRow {
  const span = rawSpan(fn.path.node);
  const nameId = fnNameIdentifier(fn);
  return {
    key: keyOf(span),
    sessionId: fn.sessionId,
    kind: "function",
    name: nameId?.name ?? "",
    nameBinding:
      nameId?.start != null && nameId.end != null
        ? freshKey(nameId.start, nameId.end)
        : null,
    structuralHash: fn.fingerprint.structuralHash,
    internalCallees: [...fn.internalCallees]
      .map((c) => rawSpan(c.path.node))
      .filter((s): s is { start: number; end: number } => s !== null)
      .map((s) => freshKey(s.start, s.end))
      .sort(spanKeyComparator),
    scopeParent: fn.scopeParent
      ? keyOf(rawSpan(fn.scopeParent.path.node))
      : null,
    bindings: [...(fn.placeholderBindings ?? []).entries()].map(
      ([slotKey, binding]) => {
        const slot = String(slotKey);
        const declSpan = rawSpan(binding.identifier);
        return {
          slot,
          span: keyOf(declSpan),
          name: binding.identifier.name
        };
      }
    )
  };
}

function moduleBindingRow(mb: ModuleBindingNode): DumpFunctionRow {
  const span = rawSpan(mb.identifier);
  return {
    key: keyOf(span),
    sessionId: mb.sessionId,
    kind: "module-binding",
    name: mb.name,
    nameBinding: span ? freshKey(span.start, span.end) : null,
    structuralHash: mb.fingerprint?.structuralHash ?? "",
    internalCallees: [...mb.internalCallees]
      .map((c) => ("path" in c ? rawSpan(c.path.node) : rawSpan(c.identifier)))
      .filter((s): s is { start: number; end: number } => s !== null)
      .map((s) => freshKey(s.start, s.end))
      .sort(spanKeyComparator),
    scopeParent: null,
    bindings: []
  };
}

function spanKeyComparator(a: SpanKey, b: SpanKey): number {
  if (a.start !== b.start) return a.start - b.start;
  if (a.end !== b.end) return a.end - b.end;
  return a.text < b.text ? -1 : a.text > b.text ? 1 : 0;
}

/**
 * Capture the matching cascade's final pairs + rejections, span-keyed. The
 * sessionIds join to spans through the two fingerprint indexes' node maps
 * (prior side into the PRIOR text, fresh side into the fresh text); raw
 * UTF-16, converted at write time. Called once, after the last cascade
 * round, while both indexes are live.
 */
export function captureMatchDump(
  result: MatchResult,
  cascade: "function" | "binding",
  priorIndex: FingerprintIndex,
  newIndex: FingerprintIndex
): void {
  if (!artifactDump.isEnabled()) return;
  const spanOf = (
    id: string,
    index: FingerprintIndex,
    text: "fresh" | "prior"
  ): SpanKey => {
    const fn = index.functions?.get(id);
    if (fn) {
      const span = rawSpan(fn.path.node);
      return span
        ? { text, start: span.start, end: span.end }
        : { text, start: -1, end: -1 };
    }
    const mb = index.moduleBindings?.get(id);
    if (mb) {
      const span = rawSpan(mb.identifier);
      return span
        ? { text, start: span.start, end: span.end }
        : { text, start: -1, end: -1 };
    }
    return { text, start: -1, end: -1 };
  };
  for (const { prior, fresh, tier } of result.pairResolutions) {
    artifactDump.matchPairs.push({
      cascade,
      prior: spanOf(prior, priorIndex, "prior"),
      fresh: spanOf(fresh, newIndex, "fresh"),
      tier
    });
  }
  for (const { prior, kind, candidates } of result.pairRejections) {
    artifactDump.matchRejections.push({
      cascade,
      prior: spanOf(prior, priorIndex, "prior"),
      kind,
      candidates: candidates?.map((c) => spanOf(c, newIndex, "fresh"))
    });
  }
}

/**
 * Capture the library-comment regions (offsets into the MINIFIED original —
 * a third anchored text) and the Bun CJS classification's per-factory
 * records, so a Rust leg needs no pre-beautify text (00-control §3's
 * recorded decision; 07 §2 amended by WP0.2).
 */
export function captureRegionsDump(
  commentRegions: CommentRegion[] | undefined,
  classification: BunModuleClassification | null | undefined
): void {
  if (!artifactDump.isEnabled()) return;
  artifactDump.commentRegions = (commentRegions ?? []).map((r) => ({
    span: { start: r.startOffset, end: r.endOffset ?? -1 },
    library: r.libraryName
  }));
  // Per-factory records keyed by span in the MINIFIED text: the minified
  // handle, the factory body's span, and the cross-version join hash. The
  // vendor NAME itself is not recorded here — it is carried by the written
  // vendor manifest, which the Rust leg reads from the tree.
  artifactDump.bannerClassifications = (classification?.factories ?? []).map(
    (f) => ({
      span: { start: f.byteRange[0], end: f.byteRange[1] },
      factoryVar: f.factoryVar,
      structuralHash: f.structuralHash
    })
  );
}
