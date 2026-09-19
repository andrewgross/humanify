import { webcrypto as nodeCrypto } from "node:crypto";
import { urlAlphabet } from "./url-alphabet/index.js";
export { urlAlphabet } from "./url-alphabet/index.js";
const POOL_MULTIPLIER = 128;
let randomPool, poolCursor;
function fillRandomPool(byteCount) {
  if (!randomPool || randomPool.length < byteCount) {
    randomPool = Buffer.allocUnsafe(128 * byteCount);
    nodeCrypto.getRandomValues(randomPool);
    poolCursor = 0;
  } else if (poolCursor + byteCount > randomPool.length) {
    nodeCrypto.getRandomValues(randomPool);
    poolCursor = 0;
  }
  poolCursor += byteCount;
}
export function getRandomBytes(byteCount) {
  fillRandomPool(byteCount -= 0);
  return randomPool.subarray(poolCursor - byteCount, poolCursor);
}
export function generateRandomString(charset, targetLength, randomFunc) {
  let indexMask = (2 << 31 - Math.clz32(charset.length - 1 | 1)) - 1;
  let randomArraySize = Math.ceil(1.6 * indexMask * targetLength / charset.length);
  return (desiredLength = targetLength) => {
    let resultString = "";
    for (;;) {
      let randomArray = randomFunc(randomArraySize);
      let counter = randomArraySize;
      for (; counter--;) {
        resultString += charset[randomArray[counter] & indexMask] || "";
        if (resultString.length === desiredLength) {
          return resultString;
        }
      }
    }
  };
}
export function generateCustomAlphabet(charset, length = 21) {
  return generateRandomString(charset, length, getRandomBytes);
}
export function generateNanoId(byteCount = 21) {
  fillRandomPool(byteCount -= 0);
  let idString = "";
  for (let index = poolCursor - byteCount; index < poolCursor; index++) {
    idString += urlAlphabet[63 & randomPool[index]];
  }
  return idString;
}