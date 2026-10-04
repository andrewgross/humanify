"use strict";
var base = 10;
function twice(x) {
  return x * 2 + Number(base);
}
function tag() {
  return "mod10:";
}

module.exports = { twice, tag };
