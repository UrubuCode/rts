import { describe, test, expect } from "rts:test";
import { gc, io } from "rts";

let __out: string = "";
function p(s: string): void { __out += s + "\n"; }

// Demo: padrão manual de closure re-entrante via gc.env_*.
// Valida #195 fase 1 — a primitiva env_record permite que fase 2 do
// codegen substitua promote-to-global. Cada chamada de makeCounter aloca
// seu próprio env, então duas instâncias têm state independente
// (re-entrância). Esse é o critério de aceitação que promote-to-global
// não cumpre.

const SLOT_COUNT: i32 = 0;

// Fn lifted (codegen gerará isso automaticamente em fase 2):
// recebe env como param e opera nos slots.
function counterIncrement(env: i64): void {
  const cur = gc.env_get(env, SLOT_COUNT);
  gc.env_set(env, SLOT_COUNT, cur + 1);
}

function counterRead(env: i64): i64 {
  return gc.env_get(env, SLOT_COUNT);
}

// Factory: aloca env próprio por chamada.
function makeCounter(): i64 {
  return gc.env_alloc(1);
}

// Re-entrância: c1 e c2 são counters independentes.
const c1 = makeCounter();
const c2 = makeCounter();

counterIncrement(c1);
counterIncrement(c1);
counterIncrement(c1);
counterIncrement(c2);

const r1 = counterRead(c1);
const r2 = counterRead(c2);
p(`c1=${r1} c2=${r2}`);

gc.env_free(c1);
gc.env_free(c2);

// Padrão de loop closures: cada iteração aloca env próprio.
// Sem isso, todas as closures veriam o último valor de i.
function captureI(env: i64): i64 {
  return gc.env_get(env, 0);
}

const envs: i64 = gc.env_alloc(3); // armazena 3 handles
for (let i: i32 = 0; i < 3; i++) {
  const e = gc.env_alloc(1);
  gc.env_set(e, 0, i);
  gc.env_set(envs, i, e);
}

// Cada env preserva seu próprio i.
for (let j: i32 = 0; j < 3; j++) {
  const e = gc.env_get(envs, j);
  p(`captured[${j}]=${captureI(e)}`);
  gc.env_free(e);
}
gc.env_free(envs);

describe("fixture:gc_env_closure_pattern", () => {
  test("matches expected stdout", () => {
    expect(__out).toBe("c1=3 c2=1\ncaptured[0]=0\ncaptured[1]=1\ncaptured[2]=2\n");
  });
});
