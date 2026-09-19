import { webcrypto as crypto } from "node:crypto";
import { urlAlphabet as scopedUrlAlphabet } from "./url-alphabet/index.js";
export { urlAlphabet } from "./url-alphabet/index.js";
const POOL_SIZE_MULTIPLIER = 128;
let pool, poolOffset;
function fillPool(o) {
  if (!pool || pool.length < o) {
    pool = Buffer.allocUnsafe(128 * o);
    crypto.getRandomValues(pool);
    poolOffset = 0;
  } else if (poolOffset + o > pool.length) {
    crypto.getRandomValues(pool);
    poolOffset = 0;
  }
  poolOffset += o;
}
export function random(o) {
  fillPool(o |= 0);
  return pool.subarray(poolOffset - o, poolOffset);
}
export function customRandom(o, l, t) {
  let e = (2 << 31 - Math.clz32(o.length - 1 | 1)) - 1;
  let p = Math.ceil(1.6 * e * l / o.length);
  return (r = l) => {
    let f = "";
    for (;;) {
      let l = t(p);
      let n = p;
      for (; n--;) {
        f += o[l[n] & e] || "";
        if (f.length >= r) {
          return f;
        }
      }
    }
  };
}
export function customAlphabet(o, l = 21) {
  return customRandom(o, l, random);
}
export function nanoid(o = 21) {
  fillPool(o |= 0);
  let l = "";
  for (let t = poolOffset - o; t < poolOffset; t++) {
    l += scopedUrlAlphabet[63 & pool[t]];
  }
  return l;
}