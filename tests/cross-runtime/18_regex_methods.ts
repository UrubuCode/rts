// Cross-runtime: regex methods subset that matches all runtimes today.
const text = "Item-42 and item-99";
const basic = /item-(\d+)/i;

console.log("test=" + basic.test(text));
console.log("replace=" + text.replace(/item/gi, "unit"));
console.log("replaceAll=" + "a-b-c".replaceAll("-", "/"));
