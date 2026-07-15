// Does the pipeline AUTO-select the Bun adapter on a real input (no --bundler)?
import fs from "node:fs";
import { detectBundle } from "../../src/detection/index.js";
import { buildPipelineConfig } from "../../src/pipeline/config.js";
import { selectUnpackAdapter } from "../../src/unpack/index.js";

const code = fs.readFileSync(process.argv[2], "utf8");
const detection = detectBundle(code);
const config = buildPipelineConfig(detection, {});
const adapter = selectUnpackAdapter(config);
console.log(`bundler detected: ${config.bundlerType} (${config.bundlerTier})`);
console.log(`unpack adapter selected: ${adapter.name}`);
console.log(
  adapter.name === "bun"
    ? "=> AUTO-EXTRACTS (no --bundler bun needed)"
    : "=> would passthrough (extraction NOT auto-selected)"
);
