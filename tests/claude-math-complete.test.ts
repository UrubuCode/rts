// Full ES `Math` surface - every method and constant, with the edge cases the
// spec pins down (JS `round` is floor(x+0.5), `sign`/`max`/`min` preserve -0,
// `clz32`/`imul` are ToInt32-domain, `max`/`min`/`hypot` are variadic with
// identities, NaN/+-Infinity propagation), plus every method read as a
// FIRST-CLASS FUNCTION VALUE. Expected output captured from Node v22 running
// this exact program - not hand-written.
import { describe, test, expect } from "rts:test";

let out = "";
function p(label, v) { out += label + "=" + v + "\n"; }

// ---- constants ----
p("E", Math.E);
p("LN10", Math.LN10);
p("LN2", Math.LN2);
p("LOG10E", Math.LOG10E);
p("LOG2E", Math.LOG2E);
p("PI", Math.PI);
p("SQRT1_2", Math.SQRT1_2);
p("SQRT2", Math.SQRT2);

// ---- unary, representative point + edge ----
p("abs", Math.abs(-3.5));
p("abs.inf", Math.abs(-Infinity));
p("acos", Math.acos(0.5));
p("acos.nan", Math.acos(2));
p("acosh", Math.acosh(2));
p("acosh.nan", Math.acosh(0.5));
p("asin", Math.asin(0.5));
p("asinh", Math.asinh(1));
p("atan", Math.atan(1));
// DIVERGÊNCIA CONHECIDA, medida: `atanh(0.5)` sai 1 ULP acima do V8 —
// 0x3fe193ea7aad030b aqui, 0x3fe193ea7aad030a no Node. Não é bug nosso: as três
// formulações (`x.atanh()`, `0.5*ln((1+x)/(1-x))`, `0.5*ln_1p(2x/(1-x))`) dão
// TODAS o mesmo bit em Rust; a diferença é a libm do Rust contra a fdlibm que o
// V8 carrega. Perseguir paridade bit-a-bit em transcendental é sem fim, então a
// asserção é em 15 dígitos significativos — que é onde os dois coincidem — e a
// divergência fica escrita aqui em vez de escondida num literal ajustado.
p("atanh", Math.atanh(0.5).toPrecision(15));
p("atanh.inf", Math.atanh(1));
p("cbrt", Math.cbrt(27));
p("cbrt.neg", Math.cbrt(-8));
p("ceil", Math.ceil(1.1));
p("ceil.neg", Math.ceil(-1.1));
p("cos", Math.cos(0));
p("cosh", Math.cosh(1));
p("exp", Math.exp(1));
p("expm1", Math.expm1(1e-10));
p("floor", Math.floor(1.9));
p("floor.neg", Math.floor(-1.1));
p("fround", Math.fround(1.1));
p("fround.5", Math.fround(5.5));
p("log", Math.log(Math.E));
p("log.zero", Math.log(0));
p("log.neg", Math.log(-1));
p("log10", Math.log10(1000));
p("log1p", Math.log1p(1e-10));
p("log2", Math.log2(1024));
p("sign.pos", Math.sign(5));
p("sign.neg", Math.sign(-5));
p("sign.zero", Math.sign(0));
p("sign.nzero", 1 / Math.sign(-0));
p("sign.nan", Math.sign(NaN));
p("sin", Math.sin(0));
p("sinh", Math.sinh(1));
p("sqrt", Math.sqrt(16));
p("sqrt.nan", Math.sqrt(-1));
p("tan", Math.tan(0));
p("tanh", Math.tanh(1));
p("trunc", Math.trunc(1.9));
p("trunc.neg", Math.trunc(-1.9));

// ---- round: JS is floor(x + 0.5), NOT Rust's round-half-away-from-zero ----
p("round.0_5", Math.round(0.5));
p("round.1_5", Math.round(1.5));
p("round.2_5", Math.round(2.5));
p("round.n0_5", Math.round(-0.5));
p("round.n0_5.sign", 1 / Math.round(-0.5));
p("round.n1_5", Math.round(-1.5));
p("round.n2_5", Math.round(-2.5));
p("round.n0_4", Math.round(-0.4));
p("round.n0_4.sign", 1 / Math.round(-0.4));

// ---- binary ----
p("atan2", Math.atan2(1, 1));
p("atan2.neg", Math.atan2(-1, -1));
p("pow", Math.pow(2, 10));
p("pow.frac", Math.pow(4, 0.5));
p("pow.neg", Math.pow(2, -2));

// ---- i32 domain ----
p("imul", Math.imul(3, 4));
p("imul.wrap", Math.imul(0xFFFFFFFF, 5));
p("imul.big", Math.imul(65535, 65535));
p("imul.neg", Math.imul(-5, 12));
p("clz32.1", Math.clz32(1));
p("clz32.0", Math.clz32(0));
p("clz32.all", Math.clz32(0xFFFFFFFF));
p("clz32.neg", Math.clz32(-1));
p("clz32.2p31", Math.clz32(2147483648));

// ---- variadic max/min/hypot ----
p("max2", Math.max(1, 2));
p("max4", Math.max(1, 7, 3, 5));
p("max.nan", Math.max(1, NaN));
p("max.empty", Math.max());
p("max.zero.sign", 1 / Math.max(-0, 0));
p("min2", Math.min(1, 2));
p("min4", Math.min(4, 7, 3, 5));
p("min.nan", Math.min(1, NaN));
p("min.empty", Math.min());
p("min.zero.sign", 1 / Math.min(0, -0));
p("hypot2", Math.hypot(3, 4));
p("hypot3", Math.hypot(1, 1, 1));
p("hypot.empty", Math.hypot());
p("hypot.1", Math.hypot(-7));
p("hypot.inf", Math.hypot(Infinity, NaN));

// ---- random: only the range is deterministic ----
const r = Math.random();
p("random.range", r >= 0 && r < 1);

// ---- Math methods read as FIRST-CLASS FUNCTION VALUES ----
const fClz = Math.clz32;
p("val.clz32", fClz(1));
const fAbs = Math.abs;
p("val.abs", fAbs(-9));
const fMax = Math.max;
p("val.max", fMax(1, 7, 3, 5));
p("val.max.empty", fMax());
const fMin = Math.min;
p("val.min", fMin(4, 2));
const fHypot = Math.hypot;
p("val.hypot", fHypot(3, 4));
const fRound = Math.round;
p("val.round", fRound(-1.5));
const fPow = Math.pow;
p("val.pow", fPow(2, 8));
const fImul = Math.imul;
p("val.imul", fImul(0xFFFFFFFF, 5));
const fSign = Math.sign;
p("val.sign", fSign(-4));
const fTrunc = Math.trunc;
p("val.trunc", fTrunc(-1.9));
const fRandom = Math.random;
const r2 = fRandom();
p("val.random.range", r2 >= 0 && r2 < 1);

// ---- typeof feature detection ----
p("typeof.clz32", typeof Math.clz32);
p("typeof.nope", typeof Math.nopeNotAMember);


describe("claude-math-complete", () => {
  test("full ES Math surface matches Node", () => expect(out).toBe(
  "E=2.718281828459045\n" +
  "LN10=2.302585092994046\n" +
  "LN2=0.6931471805599453\n" +
  "LOG10E=0.4342944819032518\n" +
  "LOG2E=1.4426950408889634\n" +
  "PI=3.141592653589793\n" +
  "SQRT1_2=0.7071067811865476\n" +
  "SQRT2=1.4142135623730951\n" +
  "abs=3.5\n" +
  "abs.inf=Infinity\n" +
  "acos=1.0471975511965979\n" +
  "acos.nan=NaN\n" +
  "acosh=1.3169578969248166\n" +
  "acosh.nan=NaN\n" +
  "asin=0.5235987755982989\n" +
  "asinh=0.881373587019543\n" +
  "atan=0.7853981633974483\n" +
  "atanh=0.549306144334055\n" +
  "atanh.inf=Infinity\n" +
  "cbrt=3\n" +
  "cbrt.neg=-2\n" +
  "ceil=2\n" +
  "ceil.neg=-1\n" +
  "cos=1\n" +
  "cosh=1.5430806348152437\n" +
  "exp=2.718281828459045\n" +
  "expm1=1.00000000005e-10\n" +
  "floor=1\n" +
  "floor.neg=-2\n" +
  "fround=1.100000023841858\n" +
  "fround.5=5.5\n" +
  "log=1\n" +
  "log.zero=-Infinity\n" +
  "log.neg=NaN\n" +
  "log10=3\n" +
  "log1p=9.999999999500001e-11\n" +
  "log2=10\n" +
  "sign.pos=1\n" +
  "sign.neg=-1\n" +
  "sign.zero=0\n" +
  "sign.nzero=-Infinity\n" +
  "sign.nan=NaN\n" +
  "sin=0\n" +
  "sinh=1.1752011936438014\n" +
  "sqrt=4\n" +
  "sqrt.nan=NaN\n" +
  "tan=0\n" +
  "tanh=0.7615941559557649\n" +
  "trunc=1\n" +
  "trunc.neg=-1\n" +
  "round.0_5=1\n" +
  "round.1_5=2\n" +
  "round.2_5=3\n" +
  "round.n0_5=0\n" +
  "round.n0_5.sign=-Infinity\n" +
  "round.n1_5=-1\n" +
  "round.n2_5=-2\n" +
  "round.n0_4=0\n" +
  "round.n0_4.sign=-Infinity\n" +
  "atan2=0.7853981633974483\n" +
  "atan2.neg=-2.356194490192345\n" +
  "pow=1024\n" +
  "pow.frac=2\n" +
  "pow.neg=0.25\n" +
  "imul=12\n" +
  "imul.wrap=-5\n" +
  "imul.big=-131071\n" +
  "imul.neg=-60\n" +
  "clz32.1=31\n" +
  "clz32.0=32\n" +
  "clz32.all=0\n" +
  "clz32.neg=0\n" +
  "clz32.2p31=0\n" +
  "max2=2\n" +
  "max4=7\n" +
  "max.nan=NaN\n" +
  "max.empty=-Infinity\n" +
  "max.zero.sign=Infinity\n" +
  "min2=1\n" +
  "min4=3\n" +
  "min.nan=NaN\n" +
  "min.empty=Infinity\n" +
  "min.zero.sign=-Infinity\n" +
  "hypot2=5\n" +
  "hypot3=1.7320508075688772\n" +
  "hypot.empty=0\n" +
  "hypot.1=7\n" +
  "hypot.inf=Infinity\n" +
  "random.range=true\n" +
  "val.clz32=31\n" +
  "val.abs=9\n" +
  "val.max=7\n" +
  "val.max.empty=-Infinity\n" +
  "val.min=2\n" +
  "val.hypot=5\n" +
  "val.round=-1\n" +
  "val.pow=256\n" +
  "val.imul=-5\n" +
  "val.sign=-1\n" +
  "val.trunc=-1\n" +
  "val.random.range=true\n" +
  "typeof.clz32=function\n" +
  "typeof.nope=undefined\n"
  ));
});
