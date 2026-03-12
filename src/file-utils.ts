import { existsSync } from "node:fs";
import { err } from "./cli-error.js";

export function ensureFileExists(filename: string) {
  if (!existsSync(filename)) {
    err(`File ${filename} not found`);
  }
}
