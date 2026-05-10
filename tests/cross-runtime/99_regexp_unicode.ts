// Cross-runtime compatibility: Unicode regex and matchAll.
const text = "café λ 😀";
const words = text.match(/\p{L}+/gu) ?? [];
console.log("words=" + words.join("|"));

const chars = Array.from("A😀B".matchAll(/./gu), (m) => m[0]);
console.log("chars=" + chars.join("|"));

const named = /(?<word>\p{L}+)/u.exec("λ!");
console.log("named=" + String(named?.groups?.word));
