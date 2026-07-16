// UMA coisa: toFixed nos limites do ARGUMENTO e da magnitude — 0..100 digitos,
// a fronteira 1e21 (onde vira toString), e o argumento invalido.
// (Complementa claude-to-fixed-rounding.ts, que cobre half-way rounding.)

// --- digitos 0..10 sobre o mesmo valor ---
console.log("--- digit sweep on 1.23456789 ---");
for (let d = 0; d <= 10; d++) {
  console.log("d" + d + "=" + (1.23456789).toFixed(d));
}

// --- digitos 0..10 sobre um valor que carrega ---
console.log("--- digit sweep on 9.99999 ---");
for (let d = 0; d <= 8; d++) {
  console.log("d" + d + "=" + (9.99999).toFixed(d));
}

// --- toFixed(0) nao tem ponto decimal ---
console.log("--- toFixed(0) ---");
console.log("(1.5).toFixed(0)=" + (1.5).toFixed(0));
console.log("(0).toFixed(0)=" + (0).toFixed(0));
console.log("(-0).toFixed(0)=" + (-0).toFixed(0));
console.log("(0.4).toFixed(0)=" + (0.4).toFixed(0));
console.log("(-0.4).toFixed(0)=" + (-0.4).toFixed(0));
console.log("(-0.6).toFixed(0)=" + (-0.6).toFixed(0));
console.log("(1e20).toFixed(0)=" + (1e20).toFixed(0));

// --- toFixed() sem arg == toFixed(0) ---
console.log("--- undefined arg == 0 ---");
console.log("(3.7).toFixed()=" + (3.7).toFixed());
console.log("(3.7).toFixed(undefined)=" + (3.7).toFixed(undefined));

// --- a FRONTEIRA 1e21: >= 1e21 cai pra ToString e ignora os digitos ---
console.log("--- 1e21 boundary ---");
console.log("(1e20).toFixed(2)=" + (1e20).toFixed(2));
console.log("(1e21).toFixed(2)=" + (1e21).toFixed(2));
console.log("(1e21).toFixed(0)=" + (1e21).toFixed(0));
console.log("(1e22).toFixed(2)=" + (1e22).toFixed(2));
console.log("(9.999e20).toFixed(2)=" + (9.999e20).toFixed(2));
console.log("(-1e21).toFixed(2)=" + (-1e21).toFixed(2));
console.log("(1e21-1).toFixed(0)=" + (1e21 - 1).toFixed(0));

// --- valores muito pequenos somem em zeros ---
console.log("--- tiny values ---");
console.log("(1e-7).toFixed(2)=" + (1e-7).toFixed(2));
console.log("(1e-7).toFixed(10)=" + (1e-7).toFixed(10));
console.log("(1e-10).toFixed(5)=" + (1e-10).toFixed(5));
console.log("(-1e-7).toFixed(2)=" + (-1e-7).toFixed(2));
console.log("(5e-324).toFixed(2)=" + (5e-324).toFixed(2));
console.log("(0.000000499).toFixed(6)=" + (0.000000499).toFixed(6));

// --- digitos altos (20 e 100 sao os limites da spec) ---
console.log("--- high digit counts ---");
console.log("(1).toFixed(20)=" + (1).toFixed(20));
console.log("(0.1).toFixed(20)=" + (0.1).toFixed(20));
console.log("(1).toFixed(100)=" + (1).toFixed(100));
console.log("(0.5).toFixed(50)=" + (0.5).toFixed(50));

// --- argumento fora de [0,100] lanca RangeError ---
console.log("--- argument bounds ---");
try {
  console.log("(1).toFixed(101)=" + (1).toFixed(101));
} catch (e) {
  console.log("(1).toFixed(101) threw=" + (e instanceof RangeError));
}
try {
  console.log("(1).toFixed(-1)=" + (1).toFixed(-1));
} catch (e) {
  console.log("(1).toFixed(-1) threw=" + (e instanceof RangeError));
}

// --- argumento fracionario trunca ---
console.log("--- fractional arg truncates ---");
console.log("(1.23456).toFixed(2.9)=" + (1.23456).toFixed(2.9));
console.log("(1.23456).toFixed(3.1)=" + (1.23456).toFixed(3.1));

// --- nao-finitos ignoram digitos ---
console.log("--- non-finite ---");
console.log("NaN.toFixed(2)=" + (NaN).toFixed(2));
console.log("Infinity.toFixed(2)=" + (Infinity).toFixed(2));
console.log("(-Infinity).toFixed(0)=" + (-Infinity).toFixed(0));
