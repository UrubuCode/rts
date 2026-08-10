import { describe, test, expect } from "rts:test";

// Um shift de bigint não tem largura e não passa por ToInt32: `1n << 64n` é o
// número, onde `1 << 64` é `1`. Antes de 2026-08-10 o entry point de `<<`, `>>`,
// `**` e `~` não perguntava se o operando era bigint, caía na conversão de
// Number e respondia 0 (to_int32 zera NaN) ou NaN (powf propaga). O que pinamos
// aqui é a pergunta existir, não a aritmética — `BigInt::shl/shr/pow` já eram
// testados em bigint/.

let out: string = "";
function print(v: string): void { out += v + "\n"; }

print("shl=" + (1n << 10n));
print("shl64=" + (1n << 64n));       // passa de 64 bits: 1 << 64 daria 1
print("shr=" + (1024n >> 5n));
print("shr_neg_arg=" + (1024n >> -2n));  // contagem negativa inverte a direção
print("shr_neg_val=" + (-5n >> 1n));     // arredonda para -infinito, não para zero
print("pow=" + (2n ** 10n));
print("pow_grande=" + (3n ** 50n));
print("not=" + (~1n));               // sobre o valor inteiro, não sobre 32 bits
print("num=" + Number(5n));          // Number(x) é ToNumeric; ToNumber recusaria

// Uma contagem que o resultado não caberia RECUSA, e recusa lançando: o operando
// direito de `<<` e de `**` é uma contagem, então `1n << 2n**40n` é uma linha e um
// terabyte de dígitos. Responder `undefined` daria ao programa um valor por um
// pedido que a máquina não honrou.
function porque(f: () => any): string {
  try {
    f();
    return "nao lancou";
  } catch (e: any) {
    return (e instanceof RangeError ? "RangeError" : "outro") + ": " + e.message;
  }
}

print("shl_demais=" + porque(() => 1n << (2n ** 40n)));
print("pow_demais=" + porque(() => 2n ** (2n ** 40n)));
print("pow_negativo=" + porque(() => 2n ** -1n));
// Um shift à DIREITA grande demais satura em vez de recusar: ele só perde bits,
// então a contagem além da largura responde o mesmo que a contagem da largura.
print("shr_demais=" + (-1n >> (2n ** 40n)));

describe("shift, expoente e ~ alcançam o caminho de bigint", () => {
  test("matches expected stdout", () =>
    expect(out).toBe(
      "shl=1024\n" +
      "shl64=18446744073709551616\n" +
      "shr=32\n" +
      "shr_neg_arg=4096\n" +
      "shr_neg_val=-3\n" +
      "pow=1024\n" +
      "pow_grande=717897987691852588770249\n" +
      "not=-2\n" +
      "num=5\n" +
      "shl_demais=RangeError: Maximum BigInt size exceeded\n" +
      "pow_demais=RangeError: Maximum BigInt size exceeded\n" +
      "pow_negativo=RangeError: Exponent must be non-negative\n" +
      "shr_demais=-1\n"
    )
  );
});
