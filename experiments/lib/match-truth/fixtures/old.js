// The OLD version of the fixed two-version fixture (exp092). Ground
// truth is known BY CONSTRUCTION (see score.test.ts and the README):
//
//   keepExact    — byte-identical in both versions          → MUST match
//   keepWrapper  — same body, arrow head in this version    → MUST match
//   keepRenamed  — same body, parameters renamed in new     → MUST match
//   changedSmall — one-line edit between the versions       → SHOULD match
//   removedOld   — only in this version                     → unmatched
function keepExact(alpha, beta) {
  return alpha + beta;
}
var keepWrapper = (one, two) => {
  return keepExact(one, two) + 1;
};
function keepRenamed(xx, yy) {
  return xx * yy + 7;
}
function changedSmall(n) {
  return n * 2 + 50;
}
function removedOld(z) {
  return z * 9;
}
