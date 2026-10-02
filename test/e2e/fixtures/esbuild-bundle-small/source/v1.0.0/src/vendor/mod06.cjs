"use strict";
var base = 6;
function twice(x) {
  return x * 2 + Number(base);
}
function tag() {
  return "mod06:";
}

module.exports = { twice, tag };
