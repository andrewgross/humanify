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
