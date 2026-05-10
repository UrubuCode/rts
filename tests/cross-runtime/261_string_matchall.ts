// Cross-runtime: String.prototype.matchAll with captures and named groups.
const re = /(?<l>[a-z]+)-(\d+)/g;
const text = "aa-10 bb-20";
const rows = Array.from(text.matchAll(re)).map((m) => m[0] + ":" + m[1] + ":" + m[2] + ":" + m.groups?.l);
console.log("rows=" + rows.join("|"));
console.log("lastIndex=" + re.lastIndex);
