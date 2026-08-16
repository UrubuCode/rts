// Cross-runtime: BigInt typed arrays wrap values to their signed or unsigned 64-bit ranges.
const signed = new BigInt64Array([0xffffffffffffffffn, 0x8000000000000000n, 0x7fffffffffffffffn]);
const unsigned = new BigUint64Array([-1n, -2n, 0x10000000000000001n]);
console.log([...signed].map(String).join(","));
console.log([...unsigned].map(String).join(","));

