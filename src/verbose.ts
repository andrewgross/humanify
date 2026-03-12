type ConsoleLogArgs = Parameters<typeof console.log>;

class VerboseLogger {
  private _level = 0;
  private _output: ((text: string) => void) | null = null;

  get level() {
    return this._level;
  }
  set level(v: number) {
    this._level = Math.min(Math.max(v, 0), 2);
  }

  get enabled() {
    return this.level >= 1;
  }

  /** Redirect verbose output to a custom writer (e.g., log file) */
  setOutput(writer: (text: string) => void): void {
    this._output = writer;
  }

  /** Reset output to default (console.log) */
  resetOutput(): void {
    this._output = null;
  }

  private emit(msg: string): void {
    if (this._output) {
      this._output(msg);
    } else {
      console.log(msg);
    }
  }

  log(...args: ConsoleLogArgs) {
    if (this.level >= 1) {
      const timestamp = new Date()
        .toISOString()
        .replace(/T/, " ")
        .replace(/\..+/, "");
      this.emit(
        `[${timestamp}]  ${args.map((a) => (typeof a === "string" ? a : JSON.stringify(a))).join(" ")}`
      );
    }
  }

  /** Log at debug level (level >= 2 / -vv) */
  debug(...args: ConsoleLogArgs) {
    if (this.level >= 2) {
      const timestamp = new Date()
        .toISOString()
        .replace(/T/, " ")
        .replace(/\..+/, "");
      this.emit(
        `[${timestamp}]  ${args.map((a) => (typeof a === "string" ? a : JSON.stringify(a))).join(" ")}`
      );
    }
  }
}

export const verbose = new VerboseLogger();
