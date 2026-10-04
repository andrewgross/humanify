"use strict";
var base = 2;
function twice(x) {
  return x * 2 + Number(base);
}
function tag() {
  return "mod02:";
}

module.exports = { twice, tag };
