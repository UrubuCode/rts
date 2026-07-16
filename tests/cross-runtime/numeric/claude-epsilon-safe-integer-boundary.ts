// UMA coisa: aritmetica de BOUNDARY com Number.EPSILON e MAX_SAFE_INTEGER.
// Onde o double para de contar de 1 em 1 e onde EPSILON deixa de separar.

// --- as constantes em si ---
console.log("EPSILON=" + Number.EPSILON);
console.log("MAX_SAFE=" + Number.MAX_SAFE_INTEGER);
console.log("MIN_SAFE=" + Number.MIN_SAFE_INTEGER);
console.log("MAX_VALUE=" + Number.MAX_VALUE);
console.log("MIN_VALUE=" + Number.MIN_VALUE);

// --- EPSILON: 1+EPSILON e' o proximo double depois de 1 ---
console.log("1+EPS>1=" + (1 + Number.EPSILON > 1));
console.log("1+EPS/2>1=" + (1 + Number.EPSILON / 2 > 1));
console.log("1+EPS/2===1=" + (1 + Number.EPSILON / 2 === 1));
console.log("1+EPS-1=" + (1 + Number.EPSILON - 1));
console.log("EPS===2**-52=" + (Number.EPSILON === Math.pow(2, -52)));

// --- EPSILON nao escala: perto de 2 o gap dobra ---
console.log("2+EPS>2=" + (2 + Number.EPSILON > 2));
console.log("2+EPS===2=" + (2 + Number.EPSILON === 2));
console.log("2+2*EPS>2=" + (2 + 2 * Number.EPSILON > 2));
console.log("0.5+EPS>0.5=" + (0.5 + Number.EPSILON > 0.5));

// --- o classico 0.1+0.2 vs EPSILON ---
console.log("0.1+0.2=" + (0.1 + 0.2));
console.log("0.1+0.2===0.3=" + (0.1 + 0.2 === 0.3));
console.log("diff=" + Math.abs(0.1 + 0.2 - 0.3));
console.log("diff<EPS=" + (Math.abs(0.1 + 0.2 - 0.3) < Number.EPSILON));

// --- MAX_SAFE_INTEGER: onde +1 para de funcionar ---
const MS = Number.MAX_SAFE_INTEGER;
console.log("MS+1=" + (MS + 1));
console.log("MS+2=" + (MS + 2));
console.log("MS+1===MS+2=" + (MS + 1 === MS + 2));
console.log("MS+3=" + (MS + 3));
console.log("MS+4=" + (MS + 4));
console.log("MS+3===MS+4=" + (MS + 3 === MS + 4));
console.log("MS*2=" + MS * 2);

// --- isSafeInteger na fronteira ---
console.log("isSafe(MS)=" + Number.isSafeInteger(MS));
console.log("isSafe(MS+1)=" + Number.isSafeInteger(MS + 1));
console.log("isSafe(MS+2)=" + Number.isSafeInteger(MS + 2));
console.log("isSafe(2**53)=" + Number.isSafeInteger(Math.pow(2, 53)));
console.log("isSafe(2**53-1)=" + Number.isSafeInteger(Math.pow(2, 53) - 1));
console.log("isSafe(MIN_SAFE)=" + Number.isSafeInteger(Number.MIN_SAFE_INTEGER));
console.log("isSafe(MIN_SAFE-1)=" + Number.isSafeInteger(Number.MIN_SAFE_INTEGER - 1));

// --- MIN_SAFE espelha ---
const mS = Number.MIN_SAFE_INTEGER;
console.log("mS-1=" + (mS - 1));
console.log("mS-2=" + (mS - 2));
console.log("mS-1===mS-2=" + (mS - 1 === mS - 2));

// --- 2**53 exato: pares sobrevivem, impares somem ---
const P53 = Math.pow(2, 53);
console.log("P53=" + P53);
console.log("P53+1=" + (P53 + 1));
console.log("P53+1===P53=" + (P53 + 1 === P53));
console.log("P53+2=" + (P53 + 2));
console.log("P53+2===P53=" + (P53 + 2 === P53));
console.log("P53+3=" + (P53 + 3));

// --- overflow para Infinity no topo ---
console.log("MAX_VALUE*2=" + Number.MAX_VALUE * 2);
console.log("MAX_VALUE+1e291=" + (Number.MAX_VALUE + 1e291));
console.log("MAX_VALUE+1e292=" + (Number.MAX_VALUE + 1e292));
console.log("isFinite(MAX*1.0000001)=" + isFinite(Number.MAX_VALUE * 1.0000001));

// --- underflow no fundo (MIN_VALUE e' subnormal) ---
console.log("MIN_VALUE/2=" + Number.MIN_VALUE / 2);
console.log("MIN_VALUE/2===0=" + (Number.MIN_VALUE / 2 === 0));
console.log("Object.is(MIN/2,0)=" + Object.is(Number.MIN_VALUE / 2, 0));
console.log("MIN_VALUE*0.75=" + Number.MIN_VALUE * 0.75);
console.log("-MIN_VALUE/2=" + -Number.MIN_VALUE / 2);
