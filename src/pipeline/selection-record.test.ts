import assert from "node:assert";
import { describe, it } from "node:test";
import { pipelineSelectionRecord } from "./selection-record.js";

/**
 * Which adapter processed this bundle was UNRECORDED. `PipelineConfig` carries
 * the answer, and it reached only a `-v` log line and a profiler span — so a
 * committed run could not be asked "which unpack adapter ran?", and
 * `docs/responsibility.md` listed the adapter selection points as the one part
 * of the cascade story still unobservable.
 *
 * Selection is a deterministic function of the input, so this cannot differ
 * between two runs of the same bundle. It differs across PAIRS, silently, which
 * is exactly what makes a detection change hard to attribute after the fact.
 */
describe("pipelineSelectionRecord", () => {
  const cfg = {
    bundlerType: "bun" as const,
    bundlerTier: "definitive" as const,
    minifierType: "esbuild" as const,
    unpackAdapterName: "bun"
  };

  it("captures every selection the run made", () => {
    assert.deepStrictEqual(pipelineSelectionRecord(cfg), {
      bundler: "bun",
      bundlerTier: "definitive",
      minifier: "esbuild",
      unpackAdapter: "bun"
    });
  });

  it("keeps the detection TIER — a guess and a definitive match differ", () => {
    // Same bundler, different confidence. Recording only the bundler name would
    // make an "unknown"-tier guess indistinguishable from a signature match,
    // and everything downstream is conditioned on that choice.
    assert.strictEqual(
      pipelineSelectionRecord({ ...cfg, bundlerTier: "unknown" }).bundlerTier,
      "unknown"
    );
  });

  it("records a passthrough adapter as explicitly as a real one", () => {
    // "no adapter matched" is a FINDING, not an absence — it decides everything
    // downstream, and a blank field would read as "not applicable".
    const rec = pipelineSelectionRecord({
      ...cfg,
      unpackAdapterName: "passthrough"
    });
    assert.strictEqual(rec.unpackAdapter, "passthrough");
  });
});
