// Cross-runtime: logical assignments only evaluate needed right-hand sides.
let calls = 0;
const rhs = () => { calls++; return 9; };
let a: any = 0, b: any = 2, c: any = null;
a &&= rhs();
b ||= rhs();
c ??= rhs();
console.log(a, b, c, calls);

