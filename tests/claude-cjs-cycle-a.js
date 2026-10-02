module.exports = A;
var B = require("./claude-cjs-cycle-b.js");
A.sawFromB = B;
function A() {}
A.prototype.tag = "A";
