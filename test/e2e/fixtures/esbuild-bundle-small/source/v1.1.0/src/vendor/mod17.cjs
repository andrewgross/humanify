"use strict";
var base = 17;
function twice(x) {
  return x * 2 + Number(base);
}
function tag() {
  return "mod17:";
}

module.exports = { twice, tag };
