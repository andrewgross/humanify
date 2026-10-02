"use strict";
var base = 15;
function twice(x) {
  return x * 2 + Number(base);
}
function tag() {
  return "mod15:";
}

module.exports = { twice, tag };
