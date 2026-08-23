import fs from "node:fs/promises";
import path from "node:path";
import type { BundlerDetectionResult } from "../../detection/types.js";
import {
  requireFileCode,
  type UnpackAdapter,
  type UnpackInput,
  type UnpackResult
} from "../types.js";

export class PassthroughAdapter implements UnpackAdapter {
  name = "passthrough";

  supports(_detection: BundlerDetectionResult): boolean {
    // Fallback adapter — supports everything
    return true;
  }

  async unpack(input: UnpackInput, outputDir: string): Promise<UnpackResult> {
    const code = requireFileCode(input, this.name);
    await fs.mkdir(outputDir, { recursive: true });
    const outputPath = path.join(outputDir, "index.js");
    await fs.writeFile(outputPath, code);
    return {
      files: [{ path: outputPath }]
    };
  }
}
