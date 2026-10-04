"use strict";
var base = 12;
function twice(x) {
  return x * 2 + Number(base);
}
function tag() {
  return "mod12:";
}

module.exports = { twice, tag };
