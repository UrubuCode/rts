// Cross-runtime: Unicode normalization forms.
const composed = "\u00e9";
const decomposed = "e\u0301";
console.log(composed === decomposed);
console.log(composed.normalize("NFD") === decomposed);
console.log(decomposed.normalize("NFC") === composed);
console.log(decomposed.length + ":" + decomposed.normalize("NFC").length);
