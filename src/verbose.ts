type ConsoleLogArgs = Parameters<typeof console.log>;

class VerboseLogger {
  private _level = 0;

  get level() { return this._level; }
  set level(v: number) { this._level = Math.min(Math.max(v, 0), 2); }

  get enabled() { return this.level >= 1; }

  log(...args: ConsoleLogArgs) {
    if (this.level >= 1) {
      const timestamp = new Date()
        .toISOString()
        .replace(/T/, " ")
        .replace(/\..+/, "");
      console.log(`[${timestamp}] `, ...args);
    }
  }

  /** Log at debug level (level >= 2 / -vv) */
  debug(...args: ConsoleLogArgs) {
    if (this.level >= 2) {
      const timestamp = new Date()
        .toISOString()
        .replace(/T/, " ")
        .replace(/\..+/, "");
      console.log(`[${timestamp}] `, ...args);
    }
  }
}

export const verbose = new VerboseLogger();
