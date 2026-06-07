// Cross-runtime: relational operators with strings, BigInt, null, and objects.
const obj: any = { valueOf() { return 3; } };
console.log("10" < "2");
console.log("10" < 2);
console.log(null >= 0);
console.log(undefined < 1);
console.log(obj < 4);
console.log(2n < 3);
console.log(2n == 2);
console.log(2n === 2);
