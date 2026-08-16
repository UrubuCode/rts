// Cross-runtime: the string iterator combines valid pairs and leaves lone surrogates.
const s = "A\ud83d\ude00\ud800B\udc00";
const values = [...s];
console.log(values.length, values.map((x) => x.length).join(","));
console.log(values.map((x) => x.codePointAt(0)!.toString(16)).join(","));

