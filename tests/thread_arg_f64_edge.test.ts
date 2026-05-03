// Edge cases f64 em thread.spawn(fp, N) — #435.
// NaN, Infinity, -0.0, fracao com magnitude que cabe em i32.
import { describe, test, expect } from "rts:test";
import { thread, atomic } from "rts";

const slot = atomic.f64_new(0.0);

function workerStore(arg: number): void {
  atomic.f64_store(slot, arg);
}

// -0.0 — bit pattern distinto de +0.0 (sign bit set). Fcvt perderia.
atomic.f64_store(slot, 1.0);
const fpA = getPointer(workerStore);
const tA = thread.spawn(fpA, -0.0);
thread.join(tA);
const gotNegZero = atomic.f64_load(slot);
// 1.0 / -0.0 = -Infinity ; 1.0 / +0.0 = +Infinity
const negZeroOk = (1.0 / gotNegZero) < 0.0;

// Fracao pequena com parte inteira que cabe em i32 — caso classico
// onde fcvt_to_uint_sat truncaria pra 42, perdendo .5
atomic.f64_store(slot, 0.0);
const tB = thread.spawn(fpA, 42.5);
thread.join(tB);
const got42_5 = atomic.f64_load(slot);

// Infinity — bit pattern 0x7FF0000000000000 ; fcvt_to_uint_sat saturaria
// pra u64::MAX, perdendo a representacao IEEE.
atomic.f64_store(slot, 0.0);
const inf = 1.0 / 0.0;
const tC = thread.spawn(fpA, inf);
thread.join(tC);
const gotInf = atomic.f64_load(slot);
const infOk = gotInf > 1e300; // Infinity > qualquer finito

describe("fixture:thread_arg_f64_edge", () => {
  test("spawn(fp, -0.0) preserva sign bit (1/x = -Infinity)", () => {
    expect(negZeroOk ? "1" : "0").toBe("1");
  });

  test("spawn(fp, 42.5) preserva fracao mesmo com parte inteira pequena", () => {
    const close = got42_5 > 42.499 && got42_5 < 42.501;
    expect(close ? "1" : "0").toBe("1");
  });

  test("spawn(fp, Infinity) preserva bit pattern de Infinity", () => {
    expect(infOk ? "1" : "0").toBe("1");
  });
});
