module.exports = B;
var A = require("./claude-cjs-cycle-a.js");
B.sawA = typeof A;
B.sawProto = typeof A.prototype;
B.derived = Object.create(A.prototype);
function B() {}
