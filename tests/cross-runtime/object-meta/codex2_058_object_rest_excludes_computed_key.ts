// Cross-runtime: object rest excludes a computed key after evaluating it once.
let calls = 0;
const key = () => { calls++; return "b"; };
const source = { a: 1, b: 2, c: 3 };
const { [key()]: picked, ...rest } = source;
console.log(picked, JSON.stringify(rest), calls);

