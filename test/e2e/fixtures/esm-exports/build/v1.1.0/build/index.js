import { sep as a } from "node:path";
export { basename } from "node:path";
export * as posix from "node:path";
export function createStore(e) {
  let t = e;
  function n() {
    return t;
  }
  function r(e) {
    const o = t;
    t = e;
    return o;
  }
  return { get: n, set: r };
}
export class Counter {
  constructor() {
    this.n = 0;
  }
  inc() {
    return ++this.n;
  }
}
export const version = "1.1.0", limit = 4;
export let mode = "b";
const o = e => e + a;
function i(e) {
  return [e, limit, mode];
}
export { o as join, i };
export default function s(e) {
  return createStore(e ?? version);
}
