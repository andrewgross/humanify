"use strict";
// A vendored template helper with its own bundled text: text a library
// requires is that library's, and stays vendored.
var text = require("./template-text.cjs");
module.exports = {
  render: function (x) {
    return text.replace("{x}", x);
  }
};
