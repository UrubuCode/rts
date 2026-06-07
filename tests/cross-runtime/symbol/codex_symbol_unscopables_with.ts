// Cross-runtime: Symbol.unscopables affects with binding lookup.
const obj: any = { a: 1, b: 2 };
obj[Symbol.unscopables] = { a: true };
const run = Function("obj", "const a = 10; let out = ''; with (obj) { out = a + ':' + b; } return out;");
console.log(run(obj));
