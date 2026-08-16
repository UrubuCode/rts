// Cross-runtime: non-finite numbers serialize as null while signed zero becomes zero.
const values = [NaN, Infinity, -Infinity, -0, 1.5];
console.log(JSON.stringify(values));
console.log(JSON.stringify({ nan: NaN, inf: Infinity, zero: -0 }));

