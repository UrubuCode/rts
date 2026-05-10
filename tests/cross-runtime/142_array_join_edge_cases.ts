// Cross-runtime: Array.join edge cases
const arr = [1, 2, 3];
console.log("default=" + arr.join());
console.log("comma=" + arr.join(","));
console.log("dash=" + arr.join("-"));
console.log("empty=" + arr.join(""));
console.log("space=" + arr.join(" "));

const withUndef = [1, undefined, 3];
console.log("undef=" + withUndef.join(","));

const withNull = [1, null, 3];
console.log("null=" + withNull.join(","));

const mixed = [1, "", 3];
console.log("empty_str=" + mixed.join(","));

const empty: number[] = [];
console.log("empty_arr=" + empty.join(","));
