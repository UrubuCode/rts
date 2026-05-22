// Cross-runtime compatibility: String well-formed helpers.
const bad = "\uD83D";
console.log("wf=" + bad.isWellFormed());
console.log("fix=" + bad.toWellFormed());
