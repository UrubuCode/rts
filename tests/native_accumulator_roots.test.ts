import { describe, test, expect } from "rts:test";

// Os valores que um método nativo JÁ produziu vivem num `Vec<u64>` na moldura
// do próprio nativo. `roots::scan_stack` varre a pilha da MÁQUINA — o buffer do
// `Vec` está no monte do Rust, fora desse intervalo — então ninguém os via, e é
// a callback a alocar que faz a coleta cair no meio do laço.
//
// A falha não era um crash. Era uma RESPOSTA: nove de trezentas rondas voltavam
// com dados errados, porque objetos já produzidos tinham sido varridos e as
// células entregues a outra coisa. Por isso estes testes conferem o CONTEÚDO e
// não só o comprimento.

describe("o que um nativo está a acumular sobrevive à coleta", () => {
  test("map devolve os objetos que produziu, e não outros", () => {
    const xs: any = [];
    for (let i = 0; i < 500; i++) xs.push(i);
    let bad = 0;
    for (let r = 0; r < 200; r++) {
      const out: any = xs.map((v: number) => ({ v: v }));
      if (out.length !== 500) { bad = bad + 1; continue; }
      for (let i = 0; i < 500; i++) { if (out[i].v !== i) { bad = bad + 1; break; } }
    }
    expect(bad).toBe(0);
  });

  test("filter guarda os elementos que guardou", () => {
    const xs: any = [];
    for (let i = 0; i < 500; i++) xs.push({ v: i });
    let bad = 0;
    for (let r = 0; r < 200; r++) {
      const out: any = xs.filter((o: any) => ({ keep: o.v }).keep >= 0);
      if (out.length !== 500) { bad = bad + 1; continue; }
      if (out[499].v !== 499) bad = bad + 1;
    }
    expect(bad).toBe(0);
  });

  test("Array.from com um mapeador tem o mesmo buraco, e a mesma tampa", () => {
    const xs: any = [];
    for (let i = 0; i < 300; i++) xs.push(i);
    let bad = 0;
    for (let r = 0; r < 200; r++) {
      const out: any = Array.from(xs, (v: number) => ({ v: v }));
      if (out.length !== 300) { bad = bad + 1; continue; }
      if (out[299].v !== 299) bad = bad + 1;
    }
    expect(bad).toBe(0);
  });
});
