// Cross-runtime: BigInt typed array wrapping and signedness.
const signed = new BigInt64Array(3);
signed[0] = -1n;
signed[1] = 2n ** 63n;
signed[2] = -(2n ** 63n) - 1n;
console.log(Array.from(signed).map(String).join(","));

const unsigned = new BigUint64Array(3);
unsigned[0] = -1n;
unsigned[1] = 2n ** 64n;
unsigned[2] = 5n;
console.log(Array.from(unsigned).map(String).join(","));
