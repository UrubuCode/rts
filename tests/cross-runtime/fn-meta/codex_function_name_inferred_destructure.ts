// Cross-runtime: inferred function names through destructuring defaults.
const { a = function () {}, b = class {} } = {} as any;
const [c = function () {}, d = class {}] = [] as any[];
console.log(a.name);
console.log(b.name);
console.log(c.name);
console.log(d.name);
