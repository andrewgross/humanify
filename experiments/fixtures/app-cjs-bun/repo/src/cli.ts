import minimist from "minimist";

export interface CliArgs {
  verbose: boolean;
  timeout: number | undefined;
  _: string[];
}

export function parseArgs(argv: string[]): CliArgs {
  const parsed = minimist(argv, {
    boolean: ["verbose"],
    alias: { v: "verbose", t: "timeout" }
  });

  return {
    verbose: parsed.verbose ?? false,
    timeout: parsed.timeout ? Number(parsed.timeout) : undefined,
    _: parsed._
  };
}
