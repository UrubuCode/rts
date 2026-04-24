import { io, bigfloat, math } from "rts";

// Calcula pi com 30 digitos via formula de Machin:
//   pi = 16 * atan(1/5) - 4 * atan(1/239)
// usando serie de Maclaurin: atan(x) = x - x^3/3 + x^5/5 - x^7/7 + ...
// bigfloat tem precisao fixa em digitos decimais (cap em 36 — i128 so vai
// ate ~38 digitos decimais antes de estourar).

const PREC = 30;

// atan(1/n) via serie. `terms` controla quantos elementos somar —
// cada termo reduz o erro por ~n^2, entao 1/5 com 50 termos ja cobre
// bem os 30 digitos.
function atanInverse(n: i64, terms: i32): u64 {
  const zero = bigfloat.zero(PREC);
  const one = bigfloat.from_i64(1, PREC);
  const n_big = bigfloat.from_i64(n, PREC);
  const n_sq = bigfloat.mul(n_big, n_big);

  // power = 1/n — produz um handle novo cada iteracao.
  let power = bigfloat.div(one, n_big);
  // result comeca em 0 e recebe o primeiro termo via add, para nao
  // aliasar power.
  let result = bigfloat.add(zero, power);

  let sign = -1;
  let i = 1;
  while (i < terms) {
    // power /= n^2
    const next_power = bigfloat.div(power, n_sq);
    bigfloat.free(power);
    power = next_power;

    // termo = power / (2i + 1)
    const denom = bigfloat.from_i64(2 * i + 1, PREC);
    const term = bigfloat.div(power, denom);
    bigfloat.free(denom);

    const new_result = sign > 0
      ? bigfloat.add(result, term)
      : bigfloat.sub(result, term);
    bigfloat.free(result);
    bigfloat.free(term);
    result = new_result;

    sign = -sign;
    i = i + 1;
  }

  bigfloat.free(zero);
  bigfloat.free(one);
  bigfloat.free(n_big);
  bigfloat.free(n_sq);
  bigfloat.free(power);
  return result;
}

// atan(1/5) perde 2*log10(5) ~= 1.4 digitos por termo; 80 termos cobrem
// com folga os 30 digitos de precisao.
const atan_1_5 = atanInverse(5, 80);
// atan(1/239) perde 2*log10(239) ~= 4.8 digitos por termo; 20 ja sobra.
const atan_1_239 = atanInverse(239, 20);

const sixteen = bigfloat.from_i64(16, PREC);
const four = bigfloat.from_i64(4, PREC);

const a = bigfloat.mul(sixteen, atan_1_5);
const b = bigfloat.mul(four, atan_1_239);
const pi = bigfloat.sub(a, b);

io.print(`pi (bigfloat, ${PREC} digits):`);
io.print(bigfloat.to_string(pi));
io.print(`pi (f64):`);
io.print(`${math.PI}`);
io.print(`diff to f64: ${bigfloat.to_f64(pi) - math.PI}`);

bigfloat.free(atan_1_5);
bigfloat.free(atan_1_239);
bigfloat.free(sixteen);
bigfloat.free(four);
bigfloat.free(a);
bigfloat.free(b);
bigfloat.free(pi);
