// Cross-runtime: String.codePointAt
const str = "hello";
console.log("h=" + str.codePointAt(0));
console.log("e=" + str.codePointAt(1));
console.log("o=" + str.codePointAt(4));
console.log("out=" + str.codePointAt(10));

// Emoji (surrogate pair)
const emoji = "😀A";
console.log("emoji=" + emoji.codePointAt(0));
console.log("A=" + emoji.codePointAt(2));

// Compare with charCodeAt
console.log("charCode=" + emoji.charCodeAt(0));
