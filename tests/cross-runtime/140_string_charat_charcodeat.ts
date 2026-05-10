// Cross-runtime: String charAt and charCodeAt
const str = "hello";
console.log("charAt0=" + str.charAt(0));
console.log("charAt4=" + str.charAt(4));
console.log("charAt10=" + str.charAt(10));
console.log("charCodeAt0=" + str.charCodeAt(0));
console.log("charCodeAt4=" + str.charCodeAt(4));
console.log("charCodeAt10=" + str.charCodeAt(10));

// Unicode
const emoji = "😀";
console.log("emoji_charAt=" + emoji.charAt(0));
console.log("emoji_code=" + emoji.charCodeAt(0));

// Bracket notation
console.log("bracket=" + str[0]);
console.log("bracket_out=" + str[10]);
