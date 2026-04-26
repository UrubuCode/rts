import { describe, test, expect } from "rts:test";
import { gc, io } from "rts";

let __out: string = "";
function p(s: string): void { __out += s + "\n"; }

// Caso base: alloc, set, get, free.
const env1 = gc.env_alloc(4);
gc.env_set(env1, 0, 10);
gc.env_set(env1, 1, 20);
gc.env_set(env1, 2, 30);
p(`a=${gc.env_get(env1, 0)} b=${gc.env_get(env1, 1)} c=${gc.env_get(env1, 2)} d=${gc.env_get(env1, 3)}`);

// Slots fora do range retornam 0.
p(`out=${gc.env_get(env1, 99)}`);

// Set out-of-range falha (retorna 0).
p(`set_oor=${gc.env_set(env1, 99, 42)}`);

// Free funciona.
p(`free=${gc.env_free(env1)}`);

// Após free, get retorna 0.
p(`after_free=${gc.env_get(env1, 0)}`);

// Free de handle inválido retorna 0.
p(`free_inv=${gc.env_free(0)}`);

// Múltiplos envs independentes — cada um com seu próprio storage.
const envA = gc.env_alloc(2);
const envB = gc.env_alloc(2);
gc.env_set(envA, 0, 100);
gc.env_set(envB, 0, 200);
p(`A=${gc.env_get(envA, 0)} B=${gc.env_get(envB, 0)}`);
gc.env_free(envA);
gc.env_free(envB);

describe("fixture:gc_env_basic", () => {
  test("matches expected stdout", () => {
    expect(__out).toBe("a=10 b=20 c=30 d=0\nout=0\nset_oor=0\nfree=1\nafter_free=0\nfree_inv=0\nA=100 B=200\n");
  });
});
