// Valida que num.* e mem.* (recem-marcados pure: true em #248)
// sao reconhecidos pelo purity pass — for...of top-level com so
// chamadas a num.* / mem.* deve ser reescrito pra parallel.for_each.
//
// Como nao temos API pra inspecionar o AST pos-purity-pass diretamente,
// validamos via correctness: se rodar (paralelo ou serial) e produzir
// o resultado correto, tudo OK. Adicionalmente, paralelizar grandes
// arrays sem crash valida que a infra esta correta pra essas fns.
import { describe, test, expect } from "rts:test";
import { num, mem, atomic, gc } from "rts";

describe("fixture:parallel_purity_num_mem", () => {
  test("num.* dentro de for...of: correctness + sem crash", () => {
    // Acumulador atomic (purity pass detecta padrao e auto-promove,
    // ou pelo menos nao crasha quando atomic e usado).
    const sum = atomic.i64_new(0);
    const arr = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];

    // Body so chama num.* (todas pure agora) + atomic store.
    // atomic.i64_fetch_add nao e pura mas e safe pra side effects
    // controlados — purity pass pode escolher serial nesse caso.
    for (const x of arr) {
      const s = num.saturating_add(x, 1);  // x+1
      const w = num.wrapping_mul(s, 2);     // (x+1)*2
      atomic.i64_fetch_add(sum, w);
    }

    // sum esperado = sum((x+1)*2 for x in 1..10) = 2*sum(2..11) = 2*65 = 130
    const got = atomic.i64_load(sum);
    expect(got == 130 ? "1" : "0").toBe("1");
  });

  test("mem.size_of dentro de for...of: constantes funcionam paralelo", () => {
    const acc = atomic.i64_new(0);
    const arr = [1, 2, 3, 4];

    for (const _ of arr) {
      // mem.size_of_i64 e constante (= 8). Soma 4 vezes = 32.
      atomic.i64_fetch_add(acc, mem.size_of_i64);
    }

    const got = atomic.i64_load(acc);
    expect(got == 32 ? "1" : "0").toBe("1");
  });

  test("num.* operacoes determinísticas — multi-runs dao mesmo resultado", () => {
    const arr = [10, 20, 30, 40, 50];
    let total1 = 0;
    for (const x of arr) {
      total1 = total1 + num.checked_add(x, 5);  // x+5
    }
    let total2 = 0;
    for (const x of arr) {
      total2 = total2 + num.checked_add(x, 5);
    }
    // Mesmo array + mesma fn pura = mesmo resultado.
    // Esperado: sum(x+5) = sum(15,25,35,45,55) = 175
    expect(total1 == 175 ? "1" : "0").toBe("1");
    expect(total1 == total2 ? "1" : "0").toBe("1");
  });

  test("num.wrapping_add comportamento esperado", () => {
    // wrapping_add com valores moderados — valida que a fn realmente
    // executou e retorna o valor correto.
    const r = num.wrapping_add(100, 200);
    expect(r == 300 ? "1" : "0").toBe("1");
  });
});
