import type { BundlerDetectionResult } from "../../detection/types.js";
import { webcrack } from "../../plugins/webcrack.js";
import type { UnpackAdapter, UnpackResult } from "../types.js";

export class WebcrackAdapter implements UnpackAdapter {
  name = "webcrack";

  supports(detection: BundlerDetectionResult): boolean {
    const type = detection.bundler?.type;
    return type === "webpack" || type === "browserify";
  }

  async unpack(code: string, outputDir: string): Promise<UnpackResult> {
    const { files } = await webcrack(code, outputDir);
    return { files };
  }
}
