import fs from "fs/promises";
import { ensureFileExists } from "./file-utils.js";
import { webcrack } from "./plugins/webcrack.js";
import { detectLibraries } from "./library-detection/index.js";
import { verbose } from "./verbose.js";

export interface UnminifyOptions {
  afterFileWrite?: (filePath: string) => Promise<void>;
  skipLibraries?: boolean;
}

export async function unminify(
  filename: string,
  outputDir: string,
  plugins: ((code: string) => Promise<string>)[] = [],
  options?: UnminifyOptions
) {
  ensureFileExists(filename);
  const bundledCode = await fs.readFile(filename, "utf-8");
  const { files, bundleType } = await webcrack(bundledCode, outputDir);

  if (bundleType) {
    verbose.log(`Detected ${bundleType} bundle with ${files.length} modules`);
  }

  // Determine which files to process
  let filesToProcess = files;
  const skipLibraries = options?.skipLibraries ?? true;

  if (skipLibraries) {
    const detection = await detectLibraries(files);

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

      console.log(
        `Skipping ${detection.libraryFiles.size} library file${detection.libraryFiles.size > 1 ? "s" : ""}: ${libSummary}`
      );
    }

    filesToProcess = files.filter(
      (f) => !detection.libraryFiles.has(f.path)
    );
  }

  for (let i = 0; i < filesToProcess.length; i++) {
    console.log(`Processing file ${i + 1}/${filesToProcess.length}`);

    const file = filesToProcess[i];
    const code = await fs.readFile(file.path, "utf-8");

    if (code.trim().length === 0) {
      verbose.log(`Skipping empty file ${file.path}`);
      continue;
    }

    const formattedCode = await plugins.reduce(
      (p, next) => p.then(next),
      Promise.resolve(code)
    );

    verbose.log("Input: ", code);
    verbose.log("Output: ", formattedCode);

    await fs.writeFile(file.path, formattedCode);
    await options?.afterFileWrite?.(file.path);
  }

  console.log(`Done! You can find your unminified code in ${outputDir}`);
}
