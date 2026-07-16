// UMA coisa: os TRES conversores (+x, Number(x), parseInt(x)) sobre os MESMOS
// inputs, lado a lado. O ponto e' a DIVERGENCIA entre eles: parseInt e' prefix-
// parse (para no primeiro char invalido), +/Number sao whole-string.

const inputs: any[] = [
  "42",
  "42abc",
  "abc42",
  "  42  ",
  "",
  "   ",
  "3.99",
  "0x1F",
  "0b101",
  "0o17",
  "1e3",
  "1e-3",
  ".5",
  "5.",
  "+5",
  "-5",
  "Infinity",
  "-Infinity",
  "1_000",
  "12px",
  "  -7.5e2xyz",
  "0.0000001",
  "  0x10  ",
  "null",
  "true",
];

for (let i = 0; i < inputs.length; i++) {
  const s = inputs[i];
  const label = JSON.stringify(s);
  console.log(label + " unary=" + String(+s) + " Number=" + String(Number(s)) + " parseInt=" + String(parseInt(s)));
}

// --- nao-strings: os tres divergem mais ainda (parseInt faz ToString antes) ---
console.log("--- non-strings ---");
console.log("true unary=" + String(+true) + " Number=" + String(Number(true)) + " parseInt=" + String(parseInt(true as any)));
console.log("false unary=" + String(+false) + " Number=" + String(Number(false)) + " parseInt=" + String(parseInt(false as any)));
console.log("null unary=" + String(+null) + " Number=" + String(Number(null)) + " parseInt=" + String(parseInt(null as any)));
console.log("undefined unary=" + String(+undefined) + " Number=" + String(Number(undefined)) + " parseInt=" + String(parseInt(undefined as any)));
console.log("emptyArr unary=" + String(+[]) + " Number=" + String(Number([])) + " parseInt=" + String(parseInt([] as any)));
console.log("arr[7] unary=" + String(+[7]) + " Number=" + String(Number([7])) + " parseInt=" + String(parseInt([7] as any)));
console.log("arr[1,2] unary=" + String(+[1, 2]) + " Number=" + String(Number([1, 2])) + " parseInt=" + String(parseInt([1, 2] as any)));

// --- parseInt e' o unico que enxerga expoente como lixo ---
console.log("--- exponent handling ---");
console.log("1e3 parseInt=" + parseInt("1e3"));
console.log("1e3 parseFloat=" + parseFloat("1e3"));
console.log("1e3 Number=" + Number("1e3"));

// --- parseInt sobre numero grande passa por String() primeiro ---
console.log("--- ToString-then-parse trap ---");
console.log("parseInt(1e21)=" + parseInt(1e21 as any));
console.log("Number(1e21)=" + Number(1e21));
console.log("parseInt(0.0000005)=" + parseInt(0.0000005 as any));
console.log("Number(0.0000005)=" + Number(0.0000005));
console.log("parseInt(1e-7)=" + parseInt(1e-7 as any));
