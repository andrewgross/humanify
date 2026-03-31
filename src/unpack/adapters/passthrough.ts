import fs from "node:fs/promises";
import path from "node:path";
import type { BundlerDetectionResult } from "../../detection/types.js";
import type { UnpackAdapter, UnpackResult } from "../types.js";

export class PassthroughAdapter implements UnpackAdapter {
  name = "passthrough";

  supports(_detection: BundlerDetectionResult): boolean {
    // Fallback adapter — supports everything
    return true;
  }

  async unpack(code: string, outputDir: string): Promise<UnpackResult> {
    await fs.mkdir(outputDir, { recursive: true });
    const outputPath = path.join(outputDir, "index.js");
    await fs.writeFile(outputPath, code);
    return {
      files: [{ path: outputPath }]
    };
  }
}
