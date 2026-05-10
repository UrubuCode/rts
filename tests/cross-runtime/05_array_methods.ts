// Cross-runtime: Array methods basicos.
const a = [1, 2, 3, 4, 5];
console.log("len=" + a.length);
console.log("indexOf(3)=" + a.indexOf(3));
console.log("indexOf(99)=" + a.indexOf(99));
console.log("includes(3)=" + a.includes(3));
console.log("includes(99)=" + a.includes(99));
console.log("slice(1,3)=" + a.slice(1, 3).join(","));
console.log("join=" + a.join("-"));
console.log("reverse=" + [3, 2, 1].reverse().join(","));
console.log("isArray(a)=" + Array.isArray(a));
console.log("isArray('x')=" + Array.isArray("x" as any));
console.log("Array.of=" + Array.of(10, 20, 30).join(","));

const evens = a.filter((n: number) => n % 2 === 0);
console.log("filter=" + evens.join(","));
const doubled = a.map((n: number) => n * 2);
console.log("map=" + doubled.join(","));
const sum = a.reduce((acc: number, n: number) => acc + n, 0);
console.log("reduce=" + sum);
