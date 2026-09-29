// The NEW version of the fixed two-version fixture (exp092) — the file
// `humanify match` takes as input. Construction notes in old.js.
function keepExact(alpha, beta) {
  return alpha + beta;
}
var keepWrapper = function (one, two) {
  return keepExact(one, two) + 1;
};
function keepRenamed(px, qy) {
  return px * qy + 7;
}
function changedSmall(n) {
  return n * 3 + 100;
}
function brandNew(a) {
  return a - 1;
}
