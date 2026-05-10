// Cross-runtime compatibility: BigInt arithmetic and width helpers.
console.log("add=" + String(10n + 5n));
console.log("mul=" + String(7n * -3n));
console.log("div=" + String(17n / 3n));
console.log("mod=" + String(17n % 3n));
console.log("pow=" + String(2n ** 8n));
console.log("cmp=" + String(10n > 9n));
console.log("asInt=" + String(BigInt.asIntN(8, 255n)));
console.log("asUint=" + String(BigInt.asUintN(8, -1n)));
