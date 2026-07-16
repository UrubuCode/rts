// Cross-runtime: JSON.parse de numeros edge (expoentes, -0, precisao, gramatica).
// --- inteiros
console.log("zero=" + JSON.parse("0"));
console.log("neg=" + JSON.parse("-5"));
console.log("big=" + JSON.parse("9007199254740991"));

// --- -0 preserva o sinal (Object.is distingue de 0)
const negZero = JSON.parse("-0");
console.log("neg_zero=" + negZero);
console.log("neg_zero_is=" + Object.is(negZero, -0));
console.log("neg_zero_eq_zero=" + (negZero === 0));
console.log("neg_zero_arr_is=" + Object.is(JSON.parse("[-0]")[0], -0));
console.log("pos_zero_is=" + Object.is(JSON.parse("0"), 0));

// --- expoentes (e/E, com e sem sinal)
console.log("exp_lower=" + JSON.parse("1e3"));
console.log("exp_upper=" + JSON.parse("1E3"));
console.log("exp_plus=" + JSON.parse("1e+3"));
console.log("exp_minus=" + JSON.parse("1e-3"));
console.log("exp_frac=" + JSON.parse("1.5e2"));
console.log("exp_zero=" + JSON.parse("5e0"));
console.log("exp_neg_base=" + JSON.parse("-2.5e3"));
console.log("exp_big=" + JSON.parse("1e21"));
console.log("exp_overflow=" + JSON.parse("1e400"));
console.log("exp_underflow=" + JSON.parse("1e-400"));

// --- fracionarios / precisao
console.log("frac=" + JSON.parse("0.5"));
console.log("precision_loss=" + JSON.parse("9007199254740993"));
console.log("many_digits=" + JSON.parse("0.1000000000000000055511151231257827"));
console.log("long_int=" + JSON.parse("123456789012345678901234567890"));
console.log("point_one=" + JSON.parse("0.1"));

// --- round-trip
console.log("roundtrip=" + (JSON.parse(JSON.stringify(0.1 + 0.2)) === 0.1 + 0.2));

// --- gramatica JSON e mais estrita que a de JS: estes devem LANCAR
function bad(src: string): string {
  try {
    JSON.parse(src);
    return "ok";
  } catch (e) {
    return "throw";
  }
}
console.log("leading_plus=" + bad("+1"));
console.log("leading_zero=" + bad("01"));
console.log("leading_dot=" + bad(".5"));
console.log("trailing_dot=" + bad("1."));
console.log("hex=" + bad("0x10"));
console.log("infinity=" + bad("Infinity"));
console.log("nan=" + bad("NaN"));
console.log("underscore=" + bad("1_000"));
console.log("empty_exp=" + bad("1e"));
console.log("double_neg=" + bad("--1"));
console.log("neg_dot=" + bad("-.5"));
console.log("whitespace_ok=" + bad("  1  "));
