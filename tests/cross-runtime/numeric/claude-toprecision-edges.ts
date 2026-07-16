// UMA coisa: Number.prototype.toPrecision nos EDGES — 1 sig fig, a virada
// fixed->exponencial (regra e < -6 || e >= p), limites 1/21/100, e undefined.

// --- 1 significant figure (o edge mais agressivo) ---
console.log("(0).toPrecision(1)=" + (0).toPrecision(1));
console.log("(1).toPrecision(1)=" + (1).toPrecision(1));
console.log("(9).toPrecision(1)=" + (9).toPrecision(1));
console.log("(9.5).toPrecision(1)=" + (9.5).toPrecision(1));
console.log("(9.99).toPrecision(1)=" + (9.99).toPrecision(1));
console.log("(99).toPrecision(1)=" + (99).toPrecision(1));
console.log("(123.456).toPrecision(1)=" + (123.456).toPrecision(1));
console.log("(0.5).toPrecision(1)=" + (0.5).toPrecision(1));
console.log("(0.05).toPrecision(1)=" + (0.05).toPrecision(1));
console.log("(1234).toPrecision(1)=" + (1234).toPrecision(1));
console.log("(-9.99).toPrecision(1)=" + (-9.99).toPrecision(1));

// --- a VIRADA pra exponencial: e >= p ---
console.log("--- flip to exponential (e >= p) ---");
console.log("(100).toPrecision(1)=" + (100).toPrecision(1));
console.log("(100).toPrecision(2)=" + (100).toPrecision(2));
console.log("(100).toPrecision(3)=" + (100).toPrecision(3));
console.log("(100).toPrecision(4)=" + (100).toPrecision(4));
console.log("(999).toPrecision(2)=" + (999).toPrecision(2));
console.log("(999).toPrecision(3)=" + (999).toPrecision(3));
console.log("(1000).toPrecision(3)=" + (1000).toPrecision(3));
console.log("(1000).toPrecision(4)=" + (1000).toPrecision(4));
console.log("(1234.5).toPrecision(2)=" + (1234.5).toPrecision(2));
console.log("(1234.5).toPrecision(5)=" + (1234.5).toPrecision(5));
console.log("(1234.5).toPrecision(6)=" + (1234.5).toPrecision(6));

// --- a VIRADA pra baixo: e < -6 ---
console.log("--- flip to exponential (e < -6) ---");
console.log("(0.000001).toPrecision(1)=" + (0.000001).toPrecision(1));
console.log("(0.0000001).toPrecision(1)=" + (0.0000001).toPrecision(1));
console.log("(0.000001).toPrecision(3)=" + (0.000001).toPrecision(3));
console.log("(0.0000001).toPrecision(3)=" + (0.0000001).toPrecision(3));
console.log("(0.00000123).toPrecision(2)=" + (0.00000123).toPrecision(2));
console.log("(0.0000123).toPrecision(2)=" + (0.0000123).toPrecision(2));

// --- arredondamento que CARREGA e muda o expoente ---
console.log("--- rounding carry changes exponent ---");
console.log("(9.99).toPrecision(2)=" + (9.99).toPrecision(2));
console.log("(99.9).toPrecision(2)=" + (99.9).toPrecision(2));
console.log("(0.0999).toPrecision(2)=" + (0.0999).toPrecision(2));
console.log("(9999).toPrecision(3)=" + (9999).toPrecision(3));
console.log("(0.00009999).toPrecision(2)=" + (0.00009999).toPrecision(2));

// --- toPrecision() sem argumento == toString() ---
console.log("--- undefined precision == toString ---");
console.log("(123.456).toPrecision()=" + (123.456).toPrecision());
console.log("(0.1).toPrecision()=" + (0.1).toPrecision());
console.log("(1e21).toPrecision()=" + (1e21).toPrecision());
console.log("(1e-7).toPrecision()=" + (1e-7).toPrecision());
console.log("(123.456).toPrecision(undefined)=" + (123.456).toPrecision(undefined));

// --- limites do argumento: 1 e 21 sao validos, 0 e 22 lancam ---
console.log("--- argument bounds ---");
console.log("(1).toPrecision(21)=" + (1).toPrecision(21));
console.log("(0.1).toPrecision(21)=" + (0.1).toPrecision(21));
try {
  console.log("(1).toPrecision(0)=" + (1).toPrecision(0));
} catch (e) {
  console.log("(1).toPrecision(0) threw=" + (e instanceof RangeError));
}
try {
  console.log("(1).toPrecision(22)=" + (1).toPrecision(22));
} catch (e) {
  console.log("(1).toPrecision(22) threw=" + (e instanceof RangeError));
}
try {
  console.log("(1).toPrecision(-1)=" + (1).toPrecision(-1));
} catch (e) {
  console.log("(1).toPrecision(-1) threw=" + (e instanceof RangeError));
}

// --- nao-finitos ignoram a precisao ---
console.log("--- non-finite ignores precision ---");
console.log("NaN.toPrecision(3)=" + (NaN).toPrecision(3));
console.log("Infinity.toPrecision(3)=" + (Infinity).toPrecision(3));
console.log("(-Infinity).toPrecision(1)=" + (-Infinity).toPrecision(1));

// --- precisao fracionaria e' truncada pra inteiro ---
console.log("--- fractional precision truncates ---");
console.log("(123.456).toPrecision(2.9)=" + (123.456).toPrecision(2.9));
console.log("(123.456).toPrecision(4.1)=" + (123.456).toPrecision(4.1));
