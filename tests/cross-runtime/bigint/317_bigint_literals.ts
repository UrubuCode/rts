// Cross-runtime: BigInt literals and equality with Number.
console.log("vals=" + [10n, 0xFFn].map(String).join(","));
console.log("eq=" + String(1n == 1));
console.log("seq=" + String(1n === 1));
