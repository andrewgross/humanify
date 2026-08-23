import type { BundlerDetectionResult } from "../../detection/types.js";
import { webcrack } from "../../plugins/webcrack.js";
import {
  requireFileCode,
  type UnpackAdapter,
  type UnpackInput,
  type UnpackResult
} from "../types.js";

export class WebcrackAdapter implements UnpackAdapter {
  name = "webcrack";

  supports(detection: BundlerDetectionResult): boolean {
    const type = detection.bundler?.type;
    return type === "webpack" || type === "browserify";
  }

  async unpack(input: UnpackInput, outputDir: string): Promise<UnpackResult> {
    const { files } = await webcrack(
      requireFileCode(input, this.name),
      outputDir
    );
    return { files };
  }
}
