import assert from "node:assert";
import { describe, it } from "node:test";
import { computeBands } from "./noise-bands.js";

describe("noise band computation", () => {
  it("takes the largest pairwise disagreement per KPI", () => {
    const bands = computeBands([
      { namingNoiseLines: 51414, novel: 4188, realLines: 416377 },
      { namingNoiseLines: 53430, novel: 4188, realLines: 416377 },
      { namingNoiseLines: 52245, novel: 4188, realLines: 416377 }
    ]);
    assert.strictEqual(bands.noiseLn, 53430 - 51414);
    assert.strictEqual(bands.novel, 0, "a repeated exact value earns band 0");
    assert.strictEqual(bands.realLn, 0);
  });

  it("refuses to claim a band from fewer than two values", () => {
    const bands = computeBands([{ namingNoiseLines: 51414 }, {}]);
    assert.strictEqual(
      bands.noiseLn,
      null,
      "one value is no floor — null renders as ±?, a claim of ignorance"
    );
  });
});
