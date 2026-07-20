// The `atomic` namespace memoizes the last resolved handle->address per thread
// so the hot path does not lock a shard mutex on every operation
// (CRANELIFT_IMPLEMENTATION.md step 4). These tests pin the cases a naive memo
// gets WRONG — all of them would corrupt values silently, not crash.
import { describe, test, expect } from "rts:test";
import { atomic } from "rts";

// Interleaving two handles of the SAME type: a one-entry memo must re-resolve on
// every switch, never serve handle A's address for handle B.
const a = atomic.i64_new(0);
const b = atomic.i64_new(0);
let i = 0;
while (i < 100) {
  atomic.i64_fetch_add(a, 1);
  atomic.i64_fetch_add(b, 10);
  i = i + 1;
}
const interleavedA = atomic.i64_load(a);
const interleavedB = atomic.i64_load(b);

// Interleaving DIFFERENT types. The memo is per type precisely so an i64
// address can never be reinterpreted as an AtomicBool (or vice versa).
const n = atomic.i64_new(5);
const flag = atomic.bool_new(false);
const fl = atomic.f64_new(1.5);
let j = 0;
while (j < 50) {
  atomic.i64_fetch_add(n, 2);
  atomic.bool_store(flag, true);
  atomic.f64_store(fl, 2.5);
  j = j + 1;
}
const mixedInt = atomic.i64_load(n);
const mixedBool = atomic.bool_load(flag);
const mixedFloat = atomic.f64_load(fl);

// An invalid handle must still return the graceful default, not a stale
// memoized pointer from an earlier real handle.
const invalid = atomic.i64_load(0);

describe("atomic pointer memo", () => {
  test("two handles of the same type stay independent", () => {
    expect(interleavedA).toBe(100);
    expect(interleavedB).toBe(1000);
  });

  test("interleaving types does not confuse the per-type memos", () => {
    expect(mixedInt).toBe(105);
    expect(mixedBool).toBe(true);
    expect(mixedFloat).toBe(2.5);
  });

  test("an invalid handle returns the default, not a stale pointer", () => {
    expect(invalid).toBe(0);
  });
});
