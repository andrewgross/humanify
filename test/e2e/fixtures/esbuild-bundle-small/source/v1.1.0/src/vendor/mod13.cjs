"use strict";
var base = 13;
function twice(x) {
  return x * 2 + Number(base);
}
function tag() {
  return "mod13:";
}

module.exports = { twice, tag };
