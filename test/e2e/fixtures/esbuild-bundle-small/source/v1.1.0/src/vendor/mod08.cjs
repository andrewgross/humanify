"use strict";
var base = 8;
function twice(x) {
  return x * 2 + Number(base);
}
function tag() {
  return "mod08:";
}

module.exports = { twice, tag };
