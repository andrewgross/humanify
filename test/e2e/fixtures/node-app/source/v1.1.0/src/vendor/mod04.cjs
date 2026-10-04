"use strict";
var base = 4;
function twice(x) {
  return x * 2 + Number(base);
}
function tag() {
  return "mod04:";
}

module.exports = { twice, tag };
