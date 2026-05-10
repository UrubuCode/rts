// Cross-runtime compatibility: TextEncoder/TextDecoder.
const enc = new TextEncoder();
const bytes = enc.encode("caf\u00e9-\u03bb");
console.log("len=" + bytes.length);
console.log("hex=" + Array.from(bytes).map((b) => b.toString(16)).join(","));

const dec = new TextDecoder("utf-8");
console.log("text=" + dec.decode(bytes));
