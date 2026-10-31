import assert from "node:assert";
import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import { describe, it } from "node:test";
import {
  computeBands,
  readNoiseBands,
  writeNoiseBands
} from "./noise-bands.js";

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

describe("bands keyed by naming mode (exp171)", () => {
  const tmp = () => {
    const d = fs.mkdtempSync(path.join(os.tmpdir(), "bands-"));
    const f = path.join(d, "noise-bands.json");
    fs.writeFileSync(
      f,
      JSON.stringify({
        provenance: {
          provisional: false,
          sources: ["default refs"],
          labels: ["ref-scratch"]
        },
        bands: { noiseLn: 1376, novel: 0 }
      })
    );
    return f;
  };
  const names = {
    provenance: {
      provisional: false,
      sources: ["names refs"],
      labels: ["ref-names"]
    },
    bands: { noiseLn: 900, novel: 0 }
  };

  it("names-mode bands are written BESIDE the default bands, never over them", () => {
    const f = tmp();
    const before = readNoiseBands("off", f);
    writeNoiseBands(names, "names", f);
    assert.deepStrictEqual(
      readNoiseBands("off", f),
      before,
      "default bands untouched"
    );
    assert.deepStrictEqual(readNoiseBands("names", f), names);
  });

  it("a mode with no measured bands reads as none — never another mode's", () => {
    const f = tmp();
    assert.strictEqual(readNoiseBands("names", f), null);
  });

  it("re-measuring the default bands keeps the names bands", () => {
    const f = tmp();
    writeNoiseBands(names, "names", f);
    const fresh = {
      provenance: { provisional: false, sources: ["re-measured"] },
      bands: { noiseLn: 1200 }
    };
    writeNoiseBands(fresh, "off", f);
    assert.deepStrictEqual(readNoiseBands("off", f), fresh);
    assert.deepStrictEqual(readNoiseBands("names", f), names);
  });
});
