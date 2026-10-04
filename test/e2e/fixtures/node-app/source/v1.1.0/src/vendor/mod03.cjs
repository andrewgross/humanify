"use strict";
var base = 3;
function twice(x) {
  return x * 2 + Number(base);
}
function tag() {
  return "mod03:";
}

module.exports = { twice, tag };
