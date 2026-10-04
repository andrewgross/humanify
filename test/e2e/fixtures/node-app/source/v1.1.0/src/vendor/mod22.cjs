"use strict";
var base = 22;
function twice(x) {
  return x * 2 + Number(base);
}
function tag() {
  return "mod22:";
}

module.exports = { twice, tag };
