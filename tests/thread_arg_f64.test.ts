// thread.spawn(fp, N) com worker(arg: number) — bit-pattern f64
// preservado via trampolim f64-aware (#247 bug A).
//
// IMPORTANTE: spawn precisa ser top-level (ou em fn helper top-level)
// pra o lifter detectar o ident `fp` como alias da user fn.
// Spawn dentro de arrow lifted (ex: callback de test()) nao usa
// trampolim e cai no path antigo onde arg f64 perde precisao.
import { describe, test, expect } from "rts:test";
import { thread, atomic } from "rts";

const fSlot = atomic.f64_new(0.0);

function workerF64(arg: number): void {
  atomic.f64_store(fSlot, arg);
}

function workerF64Double(arg: number): void {
  atomic.f64_store(fSlot, arg * 2.0);
}

// Spawns top-level — exercitam o fix.
const fp1 = getPointer(workerF64);
const t1 = thread.spawn(fp1, 3.14);
thread.join(t1);
const got1 = atomic.f64_load(fSlot);

atomic.f64_store(fSlot, 0.0);
const fp2 = getPointer(workerF64Double);
const t2 = thread.spawn(fp2, -2.5);
thread.join(t2);
const got2 = atomic.f64_load(fSlot);

atomic.f64_store(fSlot, 0.0);
// Literal com fracao garante parse como f64 no codegen — literais
// inteiros (1e10) viram i64 e nao casam o path bitcast do U64.
const t3 = thread.spawn(fp1, 9999999999.5);
thread.join(t3);
const got3 = atomic.f64_load(fSlot);

describe("fixture:thread_arg_f64", () => {
  test("spawn(fp, 3.14) preserva 3.14 (nao truncado pra 3)", () => {
    const close = got1 > 3.139 && got1 < 3.141;
    expect(close ? "1" : "0").toBe("1");
  });

  test("spawn(fp, -2.5) com worker que dobra → -5.0", () => {
    const close = got2 > -5.001 && got2 < -4.999;
    expect(close ? "1" : "0").toBe("1");
  });

  test("spawn(fp, 9999999999.5) magnitude grande f64 (com fracao)", () => {
    const close = got3 > 9.99e9 && got3 < 1.001e10;
    expect(close ? "1" : "0").toBe("1");
  });
});
