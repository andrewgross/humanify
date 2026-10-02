"use strict";
var base = 23;
function twice(x) {
  return x * 2 + Number(base);
}
function tag() {
  return "mod23:";
}

module.exports = { twice, tag };
