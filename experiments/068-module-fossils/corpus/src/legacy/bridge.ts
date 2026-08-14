// ESM importing CJS: forces interop wrapping for old.cjs
import legacy from "./old.cjs";

export function doubled(n: number): number {
  return legacy.legacyDouble(n) + legacy.legacyTag.length;
}
