// ONE thing: the five special inputs (NaN, +0, -0, +Infinity, -Infinity) run
// through EVERY unary Math method as a grid. Each of these cells is pinned by
// the spec's special-case list, unlike the digits of a general argument — the
// approximated results (pi/2 and friends) are asserted below as booleans only.

type Un = (x: number) => number;

const inputs: [string, number][] = [
  ["NaN", NaN],
  ["+0", 0],
  ["-0", -0],
  ["+Inf", Infinity],
  ["-Inf", -Infinity],
];

function grid(name: string, fn: Un): void {
  const cells: string[] = [];
  for (const pair of inputs) {
    const r = fn(pair[1]);
    cells.push(pair[0] + ":" + String(r) + (Object.is(r, -0) ? "(-0)" : ""));
  }
  console.log(name + " | " + cells.join(" | "));
}

// --- sign-preserving rounding family: every one of them keeps -0 ---
grid("abs", Math.abs);
grid("sign", Math.sign);
grid("trunc", Math.trunc);
grid("floor", Math.floor);
grid("ceil", Math.ceil);
grid("round", Math.round);

// --- roots ---
grid("sqrt", Math.sqrt);
grid("cbrt", Math.cbrt);

// --- exponential family ---
grid("exp", Math.exp);
grid("expm1", Math.expm1);

// --- logarithm family: log(+0) and log(-0) are both -Infinity ---
grid("log", Math.log);
grid("log2", Math.log2);
grid("log10", Math.log10);
grid("log1p", Math.log1p);

// --- circular: an infinite argument is a domain error, a zero is preserved ---
grid("sin", Math.sin);
grid("cos", Math.cos);
grid("tan", Math.tan);
grid("asin", Math.asin);

// --- hyperbolic: cosh is 1 at both zeros and +Infinity at both infinities ---
grid("sinh", Math.sinh);
grid("cosh", Math.cosh);
grid("tanh", Math.tanh);
grid("asinh", Math.asinh);
grid("acosh", Math.acosh);
grid("atanh", Math.atanh);

// --- bit-level unary methods coerce first, so -0 and NaN collapse to 0 ---
grid("fround", Math.fround);
grid("clz32", Math.clz32);

// --- acos and atan at the same inputs: only the NaN rows are pinned digits ---
console.log("acos(NaN)=" + String(Math.acos(NaN)));
console.log("acos(+Inf)=" + String(Math.acos(Infinity)));
console.log("acos(-Inf)=" + String(Math.acos(-Infinity)));
console.log("atan(+0)=" + String(Math.atan(0)) + " neg0:" + Object.is(Math.atan(0), -0));
console.log("atan(-0)=" + String(Math.atan(-0)) + " neg0:" + Object.is(Math.atan(-0), -0));
console.log("atan(NaN)=" + String(Math.atan(NaN)));

// --- the approximated cells, asserted as relationships instead of digits ---
console.log("atan(+Inf)_is_half_pi=" + (Math.atan(Infinity) === Math.PI / 2));
console.log("atan(-Inf)_is_neg_half_pi=" + (Math.atan(-Infinity) === -Math.PI / 2));
console.log("acos(0)_is_half_pi=" + (Math.acos(0) === Math.PI / 2));
console.log("acos(-0)_is_half_pi=" + (Math.acos(-0) === Math.PI / 2));
console.log("asin(1)_is_half_pi=" + (Math.asin(1) === Math.PI / 2));
console.log("acos(1)_is_exact_zero=" + (Math.acos(1) === 0));
console.log("acos(-1)_is_pi=" + (Math.acos(-1) === Math.PI));

// --- a missing argument is undefined, which is ToNumber NaN for all of them ---
const noArg: string[] = [];
const named: [string, Un][] = [
  ["abs", Math.abs], ["sqrt", Math.sqrt], ["log", Math.log],
  ["sign", Math.sign], ["cbrt", Math.cbrt], ["fround", Math.fround],
];
for (const pair of named) {
  const fn = pair[1] as any;
  noArg.push(pair[0] + ":" + String(fn()));
}
console.log("no_argument=" + noArg.join(","));
console.log("clz32_no_argument=" + String((Math.clz32 as any)()));
