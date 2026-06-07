// Cross-runtime: BigInt width wrapping edges.
console.log(BigInt.asUintN(8, -1n).toString());
console.log(BigInt.asIntN(8, 255n).toString());
console.log(BigInt.asUintN(0, 123n).toString());
console.log(BigInt.asIntN(4, -9n).toString());
console.log((2n ** 64n - 1n).toString(16));
