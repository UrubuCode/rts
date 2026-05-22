// Cross-runtime: denser array behavior and higher-order operations.
const values = [5, 12, 8, 130, 44];

console.log("find=" + values.find((n: number) => n > 10));
console.log("findIndex=" + values.findIndex((n: number) => n % 13 === 0));
console.log("some=" + values.some((n: number) => n < 0));
console.log("every=" + values.every((n: number) => n > 4));
console.log("flat=" + [1, [2, 3], [[4]]].flat(2).join(","));
console.log("at_neg=" + values.at(-2));
console.log("reduceRight=" + [1, 2, 3].reduceRight((acc: number, cur: number) => acc * 10 + cur, 0));
