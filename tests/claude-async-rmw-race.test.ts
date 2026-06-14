import { describe, test, expect } from "rts:test";

// Cobertura do fix de data race em RMW atômico (atomic-rmw-intrinsic).
//
// `async function` roda o corpo em spawn_blocking (worker thread tokio). 4
// tarefas disparadas antes do await rodam em paralelo. Cada uma faz
// `shared[0] = shared[0] + 1` em loop. Sem o fix, `shared[0]=shared[0]+1`
// compilava para VEC_GET + VEC_SET (duas calls, lock do shard solto entre
// elas) → read-modify-write não-atômico → incrementos perdidos (race,
// resultado < esperado e não-determinístico). Com o fix, o codegen reconhece
// o padrão e emite UM __RTS_FN_NS_COLLECTIONS_VEC_RMW (read+op+write sob um
// só lock) → atômico → total exato.
//
// Pré-computado no top-level (chamar método em test() closure pode coletar o
// handle antes do uso — ver nota em CLAUDE.md sobre GC em test closures).

const PER_TASK = 250000;
const N_TASKS = 4;

let shared: number[] = [0];

function bump(): number {
  for (let i = 0; i < PER_TASK; i++) {
    shared[0] = shared[0] + 1;
  }
  return shared[0];
}

async function task(): number {
  return bump();
}

// Dispara N tarefas ANTES de await — correm em paralelo (spawn_blocking).
const a = task();
const b = task();
const c = task();
const d = task();
await a;
await b;
await c;
await d;

const total = shared[0];

// Também valida compound assignment (Forma A: arr[i] += x) atômico.
let counter: number[] = [0];
function bumpCompound(): number {
  for (let i = 0; i < PER_TASK; i++) {
    counter[0] += 1;
  }
  return counter[0];
}
async function taskC(): number { return bumpCompound(); }
const ca = taskC();
const cb = taskC();
const cc = taskC();
const cd = taskC();
await ca; await cb; await cc; await cd;
const totalCompound = counter[0];

describe("async RMW atômico (race fix)", () => {
  test("forma explícita shared[0] = shared[0] + 1 sob async paralelo", () => {
    expect(total).toBe(PER_TASK * N_TASKS);
  });
  test("forma compound counter[0] += 1 sob async paralelo", () => {
    expect(totalCompound).toBe(PER_TASK * N_TASKS);
  });
});
