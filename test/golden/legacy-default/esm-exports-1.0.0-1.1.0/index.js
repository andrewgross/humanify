import { sep as aRenamed } from "node:path";
export { basename } from "node:path";
export * as posix from "node:path";
export function createStore(eRenamed) {
  let tRenamed = eRenamed;
  function nRenamed() {
    return tRenamed;
  }
  function rRenamed(eRenamedVal) {
    const oRenamedVal = tRenamed;
    tRenamed = eRenamedVal;
    return oRenamedVal;
  }
  return {
    get: nRenamed,
    set: rRenamed
  };
}
export class Counter {
  constructor() {
    this.n = 0;
  }
  inc() {
    return ++this.n;
  }
}
export const version = "1.1.0";
export const limit = 4;
export const mode = "b";
const oRenamed = eRenamed => eRenamed + aRenamed;
function iRenamed(eRenamed) {
  return [eRenamed, limit, mode];
}
export { oRenamed as join, iRenamed as i };
export default function sRenamed(eRenamed) {
  return createStore(eRenamed ?? version);
}