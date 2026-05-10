// Cross-runtime: String.match and search
const str = "The quick brown fox";

const match1 = str.match(/quick/);
console.log("match=" + (match1 ? match1[0] : "null"));

const match2 = str.match(/xyz/);
console.log("nomatch=" + match2);

const matchAll = str.match(/o/g);
console.log("matchall=" + (matchAll ? matchAll.join(",") : "null"));

console.log("search=" + str.search(/brown/));
console.log("search_none=" + str.search(/xyz/));
console.log("search_start=" + str.search(/^The/));
