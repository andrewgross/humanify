"use strict";
var base = 23;
function twice(x) {
  return x * 2 + Number(base);
}
function tag() {
  return "mod23:";
}
function tripled(x) {
  return x * 3 + Number(base);
}

module.exports = { twice, tag, tripled };
