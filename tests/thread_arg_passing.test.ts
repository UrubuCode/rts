// thread.spawn(fp, arg) entrega o arg ao worker. Foi quebrado por
// callconv mismatch (#206) e call_indirect Tail vs SystemV (#242).
// Apos os fixes, arg passing e robusto em varios cenarios.
import { describe, test, expect } from "rts:test";
import { thread, atomic } from "rts";

const sumI64 = atomic.i64_new(0);
function workerI64(arg: i64): void {
  atomic.i64_fetch_add(sumI64, arg);
}

function spawnAndJoinHelper(value: i64): void {
  const fp = workerI64 as unknown as number;
  const t = thread.spawn(fp, value);
  thread.join(t);
}

describe("fixture:thread_arg_passing", () => {
  test("spawn(fp, N) entrega N ao worker(arg: i64)", () => {
    atomic.i64_store(sumI64, 0);
    const fp = workerI64 as unknown as number;
    const t = thread.spawn(fp, 42);
    thread.join(t);
    const got = atomic.i64_load(sumI64);
    expect(got == 42 ? "1" : "0").toBe("1");
  });

  // OBS: spawn(fp, 3.14) com worker(arg: number) ainda nao funciona
  // — o trampolim recebe __rts_spawn_arg: i64 e a coerção integer→f64
  // perde o bit-pattern. Workaround: passar como int e dividir.
  // Bug separado do callconv original.

  test("multiplos spawns paralelos com args diferentes", () => {
    atomic.i64_store(sumI64, 0);
    const fp = workerI64 as unknown as number;
    const t1 = thread.spawn(fp, 10);
    const t2 = thread.spawn(fp, 20);
    const t3 = thread.spawn(fp, 30);
    const t4 = thread.spawn(fp, 40);
    thread.join(t1);
    thread.join(t2);
    thread.join(t3);
    thread.join(t4);
    const got = atomic.i64_load(sumI64);
    expect(got == 100 ? "1" : "0").toBe("1");
  });

  test("spawn dentro de fn helper (nao top-level) — caso #206 historico", () => {
    atomic.i64_store(sumI64, 0);
    spawnAndJoinHelper(99);
    const got = atomic.i64_load(sumI64);
    expect(got == 99 ? "1" : "0").toBe("1");
  });
});
