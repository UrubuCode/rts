// Cross-runtime: rest params in arrow function.
const f = (a: number, ...rest: number[]) => a + ":" + rest.length + ":" + rest.join(",");
console.log("a=" + f(1));
console.log("b=" + f(1, 2, 3, 4));
