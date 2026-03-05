import fs from "fs/promises";
import { ensureFileExists } from "./file-utils.js";
import { webcrack } from "./plugins/webcrack.js";
import { detectLibraries } from "./library-detection/index.js";
import type { MixedFileDetection } from "./library-detection/index.js";
import type { CommentRegion } from "./library-detection/index.js";
import { verbose } from "./verbose.js";

export interface UnminifyOptions {
  afterFileWrite?: (filePath: string) => Promise<void>;
  skipLibraries?: boolean;
  /**
   * Called before processing each file with mixed library/app code.
   * Allows the caller to configure per-file state (e.g., set comment regions
   * on a rename plugin). Called with undefined to clear after processing.
   */
  onCommentRegions?: (regions: CommentRegion[] | undefined) => void;
  /** Custom log output function (defaults to console.log) */
  log?: (message: string) => void;
}

export async function unminify(
  filename: string,
  outputDir: string,
  plugins: ((code: string) => Promise<string>)[] = [],
  options?: UnminifyOptions
) {
  const log = options?.log ?? console.log;

  ensureFileExists(filename);
  const bundledCode = await fs.readFile(filename, "utf-8");
  const { files, bundleType } = await webcrack(bundledCode, outputDir);

  if (bundleType) {
    verbose.log(`Detected ${bundleType} bundle with ${files.length} modules`);
  }

  // Determine which files to process
  let filesToProcess = files;
  let mixedFiles = new Map<string, MixedFileDetection>();
  const skipLibraries = options?.skipLibraries ?? true;

  if (skipLibraries) {
    const detection = await detectLibraries(files);
    mixedFiles = detection.mixedFiles;

    if (detection.libraryFiles.size > 0) {
      // Group by library name for logging
      const libCounts = new Map<string, number>();
      for (const det of detection.libraryFiles.values()) {
        const name = det.libraryName ?? "unknown";
        libCounts.set(name, (libCounts.get(name) ?? 0) + 1);
      }

      const libSummary = Array.from(libCounts.entries())
        .map(([name, count]) => `${name} (${count} file${count > 1 ? "s" : ""})`)
        .join(", ");

      log(
        `Skipping ${detection.libraryFiles.size} library file${detection.libraryFiles.size > 1 ? "s" : ""}: ${libSummary}`
      );
    }

    if (mixedFiles.size > 0) {
      for (const [path, mixed] of mixedFiles) {
        const libs = mixed.libraryNames.join(", ");
        log(
          `Mixed file ${path}: will skip library functions (${libs})`
        );
      }
    }

    filesToProcess = files.filter(
      (f) => !detection.libraryFiles.has(f.path)
    );
  }

  for (let i = 0; i < filesToProcess.length; i++) {
    log(`Processing file ${i + 1}/${filesToProcess.length}`);

    const file = filesToProcess[i];
    const code = await fs.readFile(file.path, "utf-8");

    if (code.trim().length === 0) {
      verbose.log(`Skipping empty file ${file.path}`);
      continue;
    }

    // Set comment regions for mixed files before running plugins
    const mixed = mixedFiles.get(file.path);
    if (mixed && options?.onCommentRegions) {
      options.onCommentRegions(mixed.regions);
    }

    const formattedCode = await plugins.reduce(
      (p, next) => p.then(next),
      Promise.resolve(code)
    );

    // Clear comment regions after processing
    if (mixed && options?.onCommentRegions) {
      options.onCommentRegions(undefined);
    }

    verbose.debug("Input: ", code.slice(0, 2000) + (code.length > 2000 ? "\n... truncated" : ""));
    verbose.debug("Output: ", formattedCode.slice(0, 2000) + (formattedCode.length > 2000 ? "\n... truncated" : ""));

    await fs.writeFile(file.path, formattedCode);
    await options?.afterFileWrite?.(file.path);
  }

  log(`Done! You can find your unminified code in ${outputDir}`);
}
