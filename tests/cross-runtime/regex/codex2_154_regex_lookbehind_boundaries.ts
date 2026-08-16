// Cross-runtime: lookbehind asserts preceding text without consuming it.
const input = "$10 €20 30";
console.log(JSON.stringify(input.match(/(?<=[$€])\d+/g)));
console.log(JSON.stringify(input.match(/(?<![$€])\b\d+/g)));

