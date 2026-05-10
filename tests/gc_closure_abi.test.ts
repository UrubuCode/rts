// Smoke test para gc.closure_* (Fase 1 do refator de #195).
// Valida que o ABI de Closure (alloc/fn_ptr/env) funciona ponta-a-ponta:
// alocar, ler fn_ptr, ler env, e que invalid handles retornam 0.
import { describe, test, expect } from "rts:test";
import { gc } from "rts";

const env: number = gc.env_alloc(2);
gc.env_set(env, 0, 42);
gc.env_set(env, 1, 99);

const fakeFnPtr: number = 0xdeadbeef;
const closure: number = gc.closure_alloc(fakeFnPtr, env);

const readFnPtr: number = gc.closure_fn_ptr(closure);
const readEnv: number = gc.closure_env(closure);
const slot0: number = gc.env_get(readEnv, 0);
const slot1: number = gc.env_get(readEnv, 1);

const invalidFnPtr: number = gc.closure_fn_ptr(0);
const invalidEnv: number = gc.closure_env(999999999);

describe("gc.closure_* ABI (#195 fase 1)", () => {
  test("closure_alloc retorna handle nao-zero", () => expect(closure !== 0).toBe(true));
  test("closure_fn_ptr le fn_ptr", () => expect(readFnPtr).toBe(fakeFnPtr));
  test("closure_env le env handle", () => expect(readEnv).toBe(env));
  test("env preservado dentro do closure", () => expect(slot0).toBe(42));
  test("env preservado slot 1", () => expect(slot1).toBe(99));
  test("closure_fn_ptr(0) retorna 0", () => expect(invalidFnPtr).toBe(0));
  test("closure_env de handle invalido retorna 0", () => expect(invalidEnv).toBe(0));
});
