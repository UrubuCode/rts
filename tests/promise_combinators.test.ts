import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// `promise.*` e `collections.vec_*` do namespace `rts` viraram a superficie que
// fica: `Promise.all/race/any/allSettled` sobre arrays JS.

// 1. Promise.all — todas resolvidas
const v1 = [Promise.resolve(10), Promise.resolve(20), Promise.resolve(30)];
const r1: any = await Promise.all(v1);
print("all_len=" + r1.length);
print("all[0]=" + r1[0]);
print("all[1]=" + r1[1]);
print("all[2]=" + r1[2]);

// 2. Promise.all com array vazio — resolve imediato com array vazio
const r2: any = await Promise.all([]);
print("all_empty=" + r2.length);

// 3. Promise.race — pega a primeira a settled. A normalizacao antiga (aceitar
// 7 ou 8) existia porque a ordem de scan do `promise.race` do namespace `rts`
// variava por plataforma. A superficie que fica NAO tem essa liberdade: a spec
// manda subscrever na ordem do iteravel, e com duas ja-resolvidas o vencedor e'
// deterministicamente a primeira. Assercao apertada, nao afrouxada.
const r3 = await Promise.race([Promise.resolve(7), Promise.resolve(8)]);
print("race=" + r3);

// 4. Promise.any — primeiro fulfill (pula rejeitadas)
const r4 = await Promise.any([Promise.reject(99), Promise.resolve(42), Promise.resolve(13)]);
print("any=" + r4);  // 42 (primeira fulfilled)

// 5. Promise.allSettled — array de objetos {status, value} | {status, reason}.
const r5: any = await Promise.allSettled([Promise.resolve(5), Promise.reject(7), Promise.resolve(11)]);
print("settled_len=" + r5.length);
print("settled[0].value=" + r5[0].value);   // 5
print("settled[1].reason=" + r5[1].reason); // 7
print("settled[2].value=" + r5[2].value);   // 11

describe("promise combinators (issue #415, F4)", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe(
      "all_len=3\nall[0]=10\nall[1]=20\nall[2]=30\nall_empty=0\nrace=7\nany=42\nsettled_len=3\nsettled[0].value=5\nsettled[1].reason=7\nsettled[2].value=11\n"
    );
  });
});
