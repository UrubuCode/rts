// Cross-runtime: nullish coalescing preserves other falsy values unlike OR.
const values: any[] = [0, "", false, NaN, null, undefined];
console.log(values.map((v) => String(v ?? "N")).join("|"));
console.log(values.map((v) => String(v || "O")).join("|"));

