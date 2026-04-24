// Pi via Machin usando BigInt nativo de JS como fixed-point decimal.
// Mesmo algoritmo do bench/pi_bigfloat.ts em RTS — serve de baseline
// justa contra o bigfloat do RTS.

const PREC = 30n;                   // digitos decimais
const SCALE = 10n ** PREC;          // multiplicador fixed-point

function atanInverse(n, terms) {
  const nBig = BigInt(n);
  const nSq = nBig * nBig;

  // power = SCALE / n   (representa 1/n)
  let power = SCALE / nBig;
  let result = power;                 // primeiro termo
  let sign = -1n;

  for (let i = 1; i < terms; i++) {
    power = power / nSq;              // /= n^2 (mantem escala)
    const denom = BigInt(2 * i + 1);
    const term = power / denom;
    result = sign > 0 ? result + term : result - term;
    sign = -sign;
  }
  return result;
}

const atan_1_5 = atanInverse(5, 80);
const atan_1_239 = atanInverse(239, 20);
const pi = 16n * atan_1_5 - 4n * atan_1_239;

// Format como decimal com PREC casas.
function fmt(x) {
  const neg = x < 0n;
  const abs = neg ? -x : x;
  const s = abs.toString().padStart(Number(PREC) + 1, "0");
  const split = s.length - Number(PREC);
  return (neg ? "-" : "") + s.slice(0, split) + "." + s.slice(split);
}

console.log(`pi (bigfloat, ${PREC} digits):`);
console.log(fmt(pi));
console.log(`pi (f64):`);
console.log(`${Math.PI}`);
