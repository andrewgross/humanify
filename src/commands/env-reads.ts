import * as fs from "node:fs";
import * as path from "node:path";
import type { Command } from "commander";
import { analyzeEnvReads } from "../env-reads/analyze.js";
import { formatEnvReadsReport } from "../env-reads/format.js";
import { listJsFilesRecursive } from "../file-utils.js";

const JS_EXTS = [".js", ".cjs", ".mjs"] as const;

/** Read the input into per-file sources: a single file as-is, or every JS
 * file under a directory (labelled relative to that directory). */
function collectInputs(
  inputPath: string
): Array<{ file: string; code: string }> {
  if (fs.statSync(inputPath).isDirectory()) {
    return listJsFilesRecursive(inputPath, inputPath, JS_EXTS)
      .sort()
      .map((rel) => ({
        file: rel,
        code: fs.readFileSync(path.join(inputPath, rel), "utf-8")
      }));
  }
  return [{ file: inputPath, code: fs.readFileSync(inputPath, "utf-8") }];
}

export function configureEnvReadsCommand(program: Command): void {
  program
    .command("env-reads")
    .description(
      "Inventory process.env / Bun.env / import.meta.env reads in a file or tree"
    )
    .argument("<path>", "A JS file, or a directory of JS files to scan")
    .option("--markdown", "Emit the report as Markdown")
    .option(
      "-o, --output <file>",
      "Write the report to a file instead of stdout"
    )
    .action(
      (input: string, opts: { markdown?: boolean; output?: string }): void => {
        if (!fs.existsSync(input)) {
          console.error(`Error: path does not exist: ${input}`);
          process.exit(1);
        }
        const inputs = collectInputs(input);
        const report = analyzeEnvReads(inputs);
        const text = formatEnvReadsReport(report, {
          markdown: !!opts.markdown
        });
        if (opts.output) {
          fs.writeFileSync(opts.output, text);
          console.error(
            `Wrote env-reads report to ${opts.output} ` +
              `(${report.byVar.length} variable(s), ${inputs.length} file(s))`
          );
        } else {
          process.stdout.write(text);
        }
      }
    );
}
