// Cross-runtime: repeat coerces fractional counts and composes with concatenation.
console.log("ab".repeat(3.9));
console.log("x".repeat("4" as any) + "!");
console.log("".repeat(100), "z".repeat(0));

