// Cada metodo de `Math` lido como VALOR deve computar EXATAMENTE o mesmo que a
// chamada estatica. O risco que isto cobre e um off-by-one entre o codigo de op
// que o front emite e o indice que o thunk decodifica: com 1 de diferenca todo
// metodo vira o VIZINHO da tabela e continua devolvendo um numero, entao nada
// quebra e so a comparacao de valor pega. Amostrado em dois pontos por metodo
// para que vizinhos que coincidem num ponto (round/sign em 0.7) nao passem.
// Comparacao via String() porque NaN !== NaN.
import { describe, test, expect } from "rts:test";

let bad = "";
function chk(name, a, b) { if (a !== b) bad += name + ": valor=" + a + " estatico=" + b + "\n"; }

const f_abs = Math.abs;
chk("abs", String(f_abs(0.7)) + "|" + String(f_abs(2.5)), String(Math.abs(0.7)) + "|" + String(Math.abs(2.5)));
const f_acos = Math.acos;
chk("acos", String(f_acos(0.7)) + "|" + String(f_acos(2.5)), String(Math.acos(0.7)) + "|" + String(Math.acos(2.5)));
const f_acosh = Math.acosh;
chk("acosh", String(f_acosh(0.7)) + "|" + String(f_acosh(2.5)), String(Math.acosh(0.7)) + "|" + String(Math.acosh(2.5)));
const f_asin = Math.asin;
chk("asin", String(f_asin(0.7)) + "|" + String(f_asin(2.5)), String(Math.asin(0.7)) + "|" + String(Math.asin(2.5)));
const f_asinh = Math.asinh;
chk("asinh", String(f_asinh(0.7)) + "|" + String(f_asinh(2.5)), String(Math.asinh(0.7)) + "|" + String(Math.asinh(2.5)));
const f_atan = Math.atan;
chk("atan", String(f_atan(0.7)) + "|" + String(f_atan(2.5)), String(Math.atan(0.7)) + "|" + String(Math.atan(2.5)));
const f_atanh = Math.atanh;
chk("atanh", String(f_atanh(0.7)) + "|" + String(f_atanh(2.5)), String(Math.atanh(0.7)) + "|" + String(Math.atanh(2.5)));
const f_cbrt = Math.cbrt;
chk("cbrt", String(f_cbrt(0.7)) + "|" + String(f_cbrt(2.5)), String(Math.cbrt(0.7)) + "|" + String(Math.cbrt(2.5)));
const f_ceil = Math.ceil;
chk("ceil", String(f_ceil(0.7)) + "|" + String(f_ceil(2.5)), String(Math.ceil(0.7)) + "|" + String(Math.ceil(2.5)));
const f_cos = Math.cos;
chk("cos", String(f_cos(0.7)) + "|" + String(f_cos(2.5)), String(Math.cos(0.7)) + "|" + String(Math.cos(2.5)));
const f_cosh = Math.cosh;
chk("cosh", String(f_cosh(0.7)) + "|" + String(f_cosh(2.5)), String(Math.cosh(0.7)) + "|" + String(Math.cosh(2.5)));
const f_exp = Math.exp;
chk("exp", String(f_exp(0.7)) + "|" + String(f_exp(2.5)), String(Math.exp(0.7)) + "|" + String(Math.exp(2.5)));
const f_expm1 = Math.expm1;
chk("expm1", String(f_expm1(0.7)) + "|" + String(f_expm1(2.5)), String(Math.expm1(0.7)) + "|" + String(Math.expm1(2.5)));
const f_f16round = Math.f16round;
chk("f16round", String(f_f16round(0.7)) + "|" + String(f_f16round(2.5)), String(Math.f16round(0.7)) + "|" + String(Math.f16round(2.5)));
const f_floor = Math.floor;
chk("floor", String(f_floor(0.7)) + "|" + String(f_floor(2.5)), String(Math.floor(0.7)) + "|" + String(Math.floor(2.5)));
const f_fround = Math.fround;
chk("fround", String(f_fround(0.7)) + "|" + String(f_fround(2.5)), String(Math.fround(0.7)) + "|" + String(Math.fround(2.5)));
const f_log = Math.log;
chk("log", String(f_log(0.7)) + "|" + String(f_log(2.5)), String(Math.log(0.7)) + "|" + String(Math.log(2.5)));
const f_log10 = Math.log10;
chk("log10", String(f_log10(0.7)) + "|" + String(f_log10(2.5)), String(Math.log10(0.7)) + "|" + String(Math.log10(2.5)));
const f_log1p = Math.log1p;
chk("log1p", String(f_log1p(0.7)) + "|" + String(f_log1p(2.5)), String(Math.log1p(0.7)) + "|" + String(Math.log1p(2.5)));
const f_log2 = Math.log2;
chk("log2", String(f_log2(0.7)) + "|" + String(f_log2(2.5)), String(Math.log2(0.7)) + "|" + String(Math.log2(2.5)));
const f_round = Math.round;
chk("round", String(f_round(0.7)) + "|" + String(f_round(2.5)), String(Math.round(0.7)) + "|" + String(Math.round(2.5)));
const f_sign = Math.sign;
chk("sign", String(f_sign(0.7)) + "|" + String(f_sign(2.5)), String(Math.sign(0.7)) + "|" + String(Math.sign(2.5)));
const f_sin = Math.sin;
chk("sin", String(f_sin(0.7)) + "|" + String(f_sin(2.5)), String(Math.sin(0.7)) + "|" + String(Math.sin(2.5)));
const f_sinh = Math.sinh;
chk("sinh", String(f_sinh(0.7)) + "|" + String(f_sinh(2.5)), String(Math.sinh(0.7)) + "|" + String(Math.sinh(2.5)));
const f_sqrt = Math.sqrt;
chk("sqrt", String(f_sqrt(0.7)) + "|" + String(f_sqrt(2.5)), String(Math.sqrt(0.7)) + "|" + String(Math.sqrt(2.5)));
const f_tan = Math.tan;
chk("tan", String(f_tan(0.7)) + "|" + String(f_tan(2.5)), String(Math.tan(0.7)) + "|" + String(Math.tan(2.5)));
const f_tanh = Math.tanh;
chk("tanh", String(f_tanh(0.7)) + "|" + String(f_tanh(2.5)), String(Math.tanh(0.7)) + "|" + String(Math.tanh(2.5)));
const f_trunc = Math.trunc;
chk("trunc", String(f_trunc(0.7)) + "|" + String(f_trunc(2.5)), String(Math.trunc(0.7)) + "|" + String(Math.trunc(2.5)));

const f_atan2 = Math.atan2;
chk("atan2", String(f_atan2(1, 2)) + "|" + String(f_atan2(-3, 4)), String(Math.atan2(1, 2)) + "|" + String(Math.atan2(-3, 4)));
const f_pow = Math.pow;
chk("pow", String(f_pow(2, 10)) + "|" + String(f_pow(3, 0.5)), String(Math.pow(2, 10)) + "|" + String(Math.pow(3, 0.5)));
const f_imul = Math.imul;
chk("imul", String(f_imul(3, 4)) + "|" + String(f_imul(65535, 65535)), String(Math.imul(3, 4)) + "|" + String(Math.imul(65535, 65535)));
const f_clz32 = Math.clz32;
chk("clz32", String(f_clz32(1)) + "|" + String(f_clz32(0)), String(Math.clz32(1)) + "|" + String(Math.clz32(0)));
const f_max = Math.max;
chk("max", String(f_max(1, 7, 3)) + "|" + String(f_max()), String(Math.max(1, 7, 3)) + "|" + String(Math.max()));
const f_min = Math.min;
chk("min", String(f_min(4, 2, 9)) + "|" + String(f_min()), String(Math.min(4, 2, 9)) + "|" + String(Math.min()));
const f_hypot = Math.hypot;
chk("hypot", String(f_hypot(3, 4)) + "|" + String(f_hypot(1, 1, 1)), String(Math.hypot(3, 4)) + "|" + String(Math.hypot(1, 1, 1)));
const f_random = Math.random;
const rv = f_random();
chk("random", String(rv >= 0 && rv < 1), "true");

describe("claude-math-fnvalue-roundtrip", () => {
  test("todo membro como valor bate com a chamada estatica", () => expect(bad).toBe(""));
});
