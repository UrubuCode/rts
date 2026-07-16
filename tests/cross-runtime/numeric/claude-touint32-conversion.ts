// UMA coisa: ToUint32 — a conversao que SO o >>> expoe. Diferente de ToInt32
// (que ja tem fixture), aqui o resultado vive em [0, 2^32-1], sem sinal.

// --- negativos viram o complemento de 2 sem sinal ---
console.log("-1>>>0=" + (-1 >>> 0));
console.log("-2>>>0=" + (-2 >>> 0));
console.log("-5>>>0=" + (-5 >>> 0));
console.log("-2147483648>>>0=" + (-2147483648 >>> 0));
console.log("-2147483647>>>0=" + (-2147483647 >>> 0));
console.log("-4294967295>>>0=" + (-4294967295 >>> 0));
console.log("-4294967296>>>0=" + (-4294967296 >>> 0));
console.log("-4294967297>>>0=" + (-4294967297 >>> 0));

// --- positivos: identidade ate 2^32-1, depois wrap modular ---
console.log("0>>>0=" + (0 >>> 0));
console.log("1>>>0=" + (1 >>> 0));
console.log("2147483647>>>0=" + (2147483647 >>> 0));
console.log("2147483648>>>0=" + (2147483648 >>> 0));
console.log("4294967295>>>0=" + (4294967295 >>> 0));
console.log("4294967296>>>0=" + (4294967296 >>> 0));
console.log("4294967297>>>0=" + (4294967297 >>> 0));
console.log("8589934592>>>0=" + (8589934592 >>> 0));
console.log("8589934593>>>0=" + (8589934593 >>> 0));

// --- ToUint32 trunca a fracao ANTES do modulo (round toward zero) ---
console.log("3.9>>>0=" + (3.9 >>> 0));
console.log("-3.9>>>0=" + (-3.9 >>> 0));
console.log("0.9>>>0=" + (0.9 >>> 0));
console.log("-0.9>>>0=" + (-0.9 >>> 0));
console.log("-0>>>0=" + (-0 >>> 0));
console.log("2147483647.9>>>0=" + (2147483647.9 >>> 0));
console.log("4294967295.999>>>0=" + (4294967295.999 >>> 0));

// --- nao-finitos e NaN viram 0 ---
console.log("NaN>>>0=" + (NaN >>> 0));
console.log("Infinity>>>0=" + (Infinity >>> 0));
console.log("-Infinity>>>0=" + (-Infinity >>> 0));
console.log("undefined>>>0=" + ((undefined as any) >>> 0));
console.log("null>>>0=" + ((null as any) >>> 0));

// --- ToUint32 vs ToInt32 no MESMO input (o contraste e' o ponto) ---
console.log("--- uint32 vs int32 ---");
const vals: number[] = [-1, -2147483648, 2147483647, 2147483648, 4294967295, 4294967296, 3.9, -3.9, NaN, Infinity, 1e10, -1e10];
for (let i = 0; i < vals.length; i++) {
  const v = vals[i];
  console.log(String(v) + " -> int32:" + (v | 0) + " uint32:" + (v >>> 0));
}

// --- shift counts sao mascarados por &31, mesmo no >>> ---
console.log("--- shift count masking ---");
console.log("-1>>>32=" + (-1 >>> 32));
console.log("-1>>>33=" + (-1 >>> 33));
console.log("-1>>>31=" + (-1 >>> 31));
console.log("1>>>0=" + (1 >>> 0));
console.log("4294967295>>>16=" + (4294967295 >>> 16));
console.log("-1>>>16=" + (-1 >>> 16));

// --- >>> vs >> sobre negativos (sign-fill vs zero-fill) ---
console.log("--- zero-fill vs sign-fill ---");
console.log("-8>>1=" + (-8 >> 1));
console.log("-8>>>1=" + (-8 >>> 1));
console.log("-1>>1=" + (-1 >> 1));
console.log("-1>>>1=" + (-1 >>> 1));
console.log("-16>>2=" + (-16 >> 2));
console.log("-16>>>2=" + (-16 >>> 2));

// --- strings passam por ToNumber antes de ToUint32 ---
console.log("--- string operands ---");
console.log("'-1'>>>0=" + (("-1" as any) >>> 0));
console.log("'0x10'>>>0=" + (("0x10" as any) >>> 0));
console.log("'abc'>>>0=" + (("abc" as any) >>> 0));
console.log("''>>>0=" + (("" as any) >>> 0));
console.log("true>>>0=" + ((true as any) >>> 0));
