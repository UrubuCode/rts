// Cross-runtime: abs/sign coerce values and retain signed-zero distinctions.
const xs: any[] = ["-3", null, -0, 0, -Infinity, NaN];
console.log(xs.map((x) => Math.abs(x)).join("|"));
console.log(xs.map((x) => String(Math.sign(x))).join("|"));
console.log(Object.is(Math.sign(-0), -0));

