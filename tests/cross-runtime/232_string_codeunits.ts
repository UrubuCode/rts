// Cross-runtime: String length counts code units
const simple = "hello";
console.log("simple=" + simple.length);

const emoji = "😀";
console.log("emoji=" + emoji.length);

const multi = "😀😀";
console.log("multi=" + multi.length);

const mixed = "a😀b";
console.log("mixed=" + mixed.length);

const empty = "";
console.log("empty=" + empty.length);
