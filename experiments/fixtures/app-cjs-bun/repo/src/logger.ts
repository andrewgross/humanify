import debug from "debug";

export function createLogger(namespace: string) {
  return debug(`app:${namespace}`);
}

export function enableLogging() {
  debug.enable("app:*");
}
