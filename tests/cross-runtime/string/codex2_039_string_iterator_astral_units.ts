// Cross-runtime: string iteration yields code points while indexing yields code units.
const s = "A😀B𝄞";
console.log(s.length, [...s].length);
console.log([...s].map((x) => x.length).join(","));
console.log([...s].map((x) => x.codePointAt(0)?.toString(16)).join(","));

