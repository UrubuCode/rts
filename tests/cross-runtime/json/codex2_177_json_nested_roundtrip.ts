// Cross-runtime: nested JSON-compatible data survives stringify/parse roundtrip.
const input = { text: "é😀", values: [0, false, null, { x: 2 }], empty: {} };
const encoded = JSON.stringify(input);
const decoded = JSON.parse(encoded);
console.log(encoded);
console.log(decoded.text, decoded.values[3].x, Object.keys(decoded.empty).length);

