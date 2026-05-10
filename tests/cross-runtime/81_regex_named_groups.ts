// Cross-runtime compatibility: named capture groups.
const match = /(?<word>ab+)/.exec("zabbbx");
console.log("word=" + String(match?.groups?.word));
