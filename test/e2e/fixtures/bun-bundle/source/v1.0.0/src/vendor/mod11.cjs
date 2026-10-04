"use strict";
var base = 11;
function twice(x) {
  return x * 2 + Number(base);
}
function tag() {
  return "mod11:";
}

module.exports = { twice, tag };
