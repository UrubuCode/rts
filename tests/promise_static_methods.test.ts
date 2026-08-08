import { describe, test, expect } from "rts:test";

let out: string = "";
function print(v: string): void { out += v + "\n"; }

// `promise.wait(h)` (espera bloqueante sobre handle) virou `await`, que e' a
// unica forma que a superficie que fica promete para ler o resultado.
// `all_handle` / `settled_handle` eram "o handle devolvido nao e' 0", isto e',
// "a combinacao produziu ALGO". Sem handles isso nao se escreve; a garantia
// equivalente — e mais forte — e' o conteudo do array resolvido, que e' o que
// se afirma agora.

const p1 = Promise.resolve(10);
const p2 = Promise.resolve(20);

const all: any = await Promise.all([p1, p2]);
print("all=" + all[0] + "," + all[1]);

// Promise.race devolve o valor da primeira a settled.
print("race=" + (await Promise.race([p1, p2])));

// Promise.any tambem — primeira a cumprir.
print("any=" + (await Promise.any([p1, p2])));

// Promise.allSettled — descritores {status, value}.
const settled: any = await Promise.allSettled([p1, p2]);
print("settled=" + settled[0].status + ":" + settled[0].value + "," + settled[1].status + ":" + settled[1].value);

describe("Promise.all/race/any/allSettled static methods (#779 / #806)", () => {
  test("matches expected stdout", () =>
    expect(out).toBe(
      "all=10,20\n" +
      "race=10\n" +
      "any=10\n" +
      "settled=fulfilled:10,fulfilled:20\n"
    )
  );
});
