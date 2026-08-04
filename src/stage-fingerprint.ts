import { createHash } from "node:crypto";

/**
 * A content fingerprint recorded at a STAGE BOUNDARY, so two runs that differ
 * can be told WHERE they first differed.
 *
 * Why this exists. A `neutrality.sh` null control — identical `src/` on both
 * legs, dependencies symlinked, both legs replaying the same cached model
 * answers — diverged once in 34 runs by 15 files and 212 lines. Timing and
 * path-dependence were both tested and refuted, and 33 later controls did not
 * reproduce it. Localising it took an evening of deduction: both legs wrote
 * ZERO cache entries, and the cache key is a sha256 over the whole request
 * including `alreadyRenamed`, so the prompts were provably identical and the
 * divergence had to be downstream of naming.
 *
 * That inference is what this makes into a lookup. A tree diff already reports
 * THAT two runs differ; nothing recorded WHERE. One hash per boundary turns
 * "somewhere in twelve stages" into "between these two".
 *
 * Sixteen hex characters: enough that a collision is not a practical concern
 * for comparing two runs of the same input, short enough to compare by eye in
 * a log line — which is the whole point, since the alternative is preserving
 * and diffing multi-GB trees.
 */
export function stageFingerprint(content: string): string {
  return createHash("sha256").update(content).digest("hex").slice(0, 16);
}
