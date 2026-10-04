"use strict";
var base = 21;
function twice(x) {
  return x * 2 + Number(base);
}
function tag() {
  return "mod21:";
}

module.exports = { twice, tag };
