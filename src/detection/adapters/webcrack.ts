import { webcrack } from "../../plugins/webcrack.js";
import type {
  BundlerAdapter,
  DetectionResult,
  UnpackResult
} from "../types.js";

export class WebcrackAdapter implements BundlerAdapter {
  name = "webcrack";

  supports(detection: DetectionResult): boolean {
    const type = detection.bundler?.type;
    return type === "webpack" || type === "browserify";
  }

  async unpack(code: string, outputDir: string): Promise<UnpackResult> {
    const { files } = await webcrack(code, outputDir);
    return { files };
  }
}
