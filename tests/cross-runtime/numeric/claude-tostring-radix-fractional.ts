// UMA coisa: Number.prototype.toString(radix) sobre valores FRACIONARIOS.
//
// NOTA DE ESCOPO: a expansao de fracoes NAO-terminantes (ex: (0.5).toString(3),
// (0.1).toString(2)) tem digitos finais implementation-defined — os engines JS
// divergem de verdade entre si nesses casos (medido). Por isso esta fixture
// cobre so o que e' DETERMINISTICO entre engines: fracoes que TERMINAM na base
// alvo (potencias de 2 em base 2/8/16) + zeros/nao-finitos/negativos.

// --- base 2: fracoes exatas (potencias de 2 => terminam) ---
console.log("0.5_b2=" + (0.5).toString(2));
console.log("0.25_b2=" + (0.25).toString(2));
console.log("0.75_b2=" + (0.75).toString(2));
console.log("0.125_b2=" + (0.125).toString(2));
console.log("0.375_b2=" + (0.375).toString(2));
console.log("0.0625_b2=" + (0.0625).toString(2));
console.log("2.5_b2=" + (2.5).toString(2));
console.log("10.625_b2=" + (10.625).toString(2));
console.log("255.5_b2=" + (255.5).toString(2));
console.log("1.5_b2=" + (1.5).toString(2));
console.log("3.75_b2=" + (3.75).toString(2));

// --- base 8: potencias de 2 terminam (8 = 2^3) ---
console.log("0.5_b8=" + (0.5).toString(8));
console.log("0.25_b8=" + (0.25).toString(8));
console.log("0.125_b8=" + (0.125).toString(8));
console.log("0.0625_b8=" + (0.0625).toString(8));
console.log("255.5_b8=" + (255.5).toString(8));
console.log("10.625_b8=" + (10.625).toString(8));
console.log("1.75_b8=" + (1.75).toString(8));

// --- base 16: potencias de 2 terminam (16 = 2^4) ---
console.log("0.5_b16=" + (0.5).toString(16));
console.log("0.25_b16=" + (0.25).toString(16));
console.log("0.0625_b16=" + (0.0625).toString(16));
console.log("0.125_b16=" + (0.125).toString(16));
console.log("255.5_b16=" + (255.5).toString(16));
console.log("10.625_b16=" + (10.625).toString(16));
console.log("4095.9375_b16=" + (4095.9375).toString(16));

// --- base 4 e 32 (tambem potencias de 2) ---
console.log("0.5_b4=" + (0.5).toString(4));
console.log("0.75_b4=" + (0.75).toString(4));
console.log("0.5_b32=" + (0.5).toString(32));
console.log("0.03125_b32=" + (0.03125).toString(32));

// --- negativos fracionarios ---
console.log("neg0.5_b2=" + (-0.5).toString(2));
console.log("neg0.75_b2=" + (-0.75).toString(2));
console.log("neg10.625_b16=" + (-10.625).toString(16));
console.log("neg255.5_b8=" + (-255.5).toString(8));
console.log("neg2.5_b2=" + (-2.5).toString(2));

// --- inteiros grandes em varias bases (sem parte fracionaria) ---
console.log("1e21_b2=" + (1e21).toString(2));
console.log("1e21_b16=" + (1e21).toString(16));
console.log("2pow53_b2=" + Math.pow(2, 53).toString(2));
console.log("2pow53_b16=" + Math.pow(2, 53).toString(16));
console.log("maxsafe_b36=" + Number.MAX_SAFE_INTEGER.toString(36));

// --- zeros e nao-finitos em base != 10 ---
console.log("zero_b2=" + (0).toString(2));
console.log("negzero_b2=" + (-0).toString(2));
console.log("zero_b36=" + (0).toString(36));
console.log("nan_b2=" + (NaN).toString(2));
console.log("nan_b36=" + (NaN).toString(36));
console.log("inf_b2=" + (Infinity).toString(2));
console.log("neginf_b36=" + (-Infinity).toString(36));

// --- radix invalido lanca RangeError ---
console.log("--- radix bounds ---");
console.log("radix2_ok=" + (5).toString(2));
console.log("radix36_ok=" + (35).toString(36));
try {
  console.log("radix1=" + (5).toString(1));
} catch (e) {
  console.log("radix1 threw=" + (e instanceof RangeError));
}
try {
  console.log("radix37=" + (5).toString(37));
} catch (e) {
  console.log("radix37 threw=" + (e instanceof RangeError));
}
try {
  console.log("radix0=" + (5).toString(0));
} catch (e) {
  console.log("radix0 threw=" + (e instanceof RangeError));
}
