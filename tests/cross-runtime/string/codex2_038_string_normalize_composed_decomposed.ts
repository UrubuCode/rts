// Cross-runtime: normalization converges canonically equivalent strings.
const composed = "\u00e9";
const decomposed = "e\u0301";
console.log(composed === decomposed);
console.log(composed.normalize("NFD") === decomposed.normalize("NFD"));
console.log(decomposed.normalize("NFC").length);

