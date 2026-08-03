/**
 * Keep the evidence when the rename invariant rejects a file.
 *
 * ## Why this exists
 *
 * The pipeline prints "output was written for inspection, but this run is
 * marked failed" — and then the split CONSUMES AND DELETES the offending file
 * (`removeConsumedSourceFile`). The claim was false, and it cost a real
 * investigation: the 2.1.198 capture was diagnosed down to a token position
 * with both contexts —
 *
 *     original: left: $4431 ; operator: "!==" ; right: $4434
 *     output:   left: $4434 ; operator: "!==" ; right: $4434
 *
 * — and the code it referred to no longer existed anywhere on disk, so the
 * next question ("which rename collapsed those two bindings?") could not be
 * asked at all.
 *
 * ## Why BOTH sides
 *
 * A capture is not visible in one file. The failure is that two bindings which
 * DIFFERED in the input became IDENTICAL in the output; reading the output
 * alone shows `b !== b`, which is indistinguishable from a legitimate NaN
 * check (`value !== value` is the standard idiom and appears throughout real
 * bundles — I mistook three such sites for the bug before realising).
 * Only the pair identifies it.
 *
 * ## Best-effort by construction
 *
 * Every operation is wrapped: preserving evidence must never turn a reportable
 * failure into a crash, because the crash would replace the diagnosis with a
 * stack trace — the same mistake `describeStructuralDivergence` had to fix.
 */
import * as fs from "node:fs";
import * as path from "node:path";
import { METADATA_DIR } from "./split/layout.js";

/** Where preserved evidence lands, under the metadata dir the split ignores. */
export const FAILED_OUTPUT_DIR = `${METADATA_DIR}/failed`;

export interface FailedOutputFile {
  /** Path of the emitted file that violated the invariant. */
  filePath: string;
  /** The file's contents BEFORE the rename pass ran. */
  originalCode: string;
  /**
   * The code the invariant check actually examined.
   *
   * THIS is the artifact to diff against `originalCode`, not the file on disk.
   * Reconcile, the deferred sweep and the family permutation all run after
   * validation and replace the output, so the written file is a different
   * thing. Measured on the exp059 capture: the checked code had 16,384,801
   * tokens and the file had 16,120,630, and diffing the file reported a
   * divergence at token 145 — a variable-declaration merge unrelated to the
   * failure — while the real one was at 308,757.
   */
  validatedCode?: string;
}

/**
 * Copy each rejected file, and its pre-rename source, under
 * `<outputDir>/.humanify/failed/`.
 *
 * Called after the rename stage and BEFORE the split, which is the only window
 * where the emitted file still exists.
 */
export function preserveFailedOutput(
  outputDir: string,
  failures: readonly FailedOutputFile[]
): void {
  if (failures.length === 0) return;
  const dest = path.join(outputDir, FAILED_OUTPUT_DIR);
  try {
    fs.mkdirSync(dest, { recursive: true });
  } catch {
    return; // cannot preserve; the failure is still reported by the caller
  }
  for (const { filePath, originalCode, validatedCode } of failures) {
    const base = path.basename(filePath);
    try {
      if (fs.existsSync(filePath)) {
        fs.copyFileSync(filePath, path.join(dest, base));
      }
    } catch {
      /* keep going: one unreadable file must not lose the others */
    }
    try {
      fs.writeFileSync(path.join(dest, `${base}.original`), originalCode);
    } catch {
      /* ditto */
    }
    // The pair that actually diffs: `.original` vs `.validated`.
    if (validatedCode !== undefined) {
      try {
        fs.writeFileSync(path.join(dest, `${base}.validated`), validatedCode);
      } catch {
        /* ditto */
      }
    }
  }
}
