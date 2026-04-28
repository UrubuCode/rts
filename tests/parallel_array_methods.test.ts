// Array.prototype.map/forEach/reduce com fn ident → parallel.* (#247 C)
import { describe, test, expect } from "rts:test";
import { gc, collections, atomic } from "rts";

function double(x: i64): i64 { return x * 2; }
function add(acc: i64, x: i64): i64 { return acc + x; }

const counter = atomic.i64_new(0);
function tally(x: i64): void {
  atomic.i64_fetch_add(counter, x);
}

const arr = [1, 2, 3, 4, 5];

// arr.map(fn) → parallel.map
const doubled = arr.map(double);
const dlen = collections.vec_len(doubled);
const d0 = collections.vec_get(doubled, 0);
const d4 = collections.vec_get(doubled, 4);

// arr.reduce(fn, init) → parallel.reduce
const sum = arr.reduce(add, 0);

// arr.forEach(fn) → parallel.for_each
atomic.i64_store(counter, 0);
arr.forEach(tally);
const counterVal = atomic.i64_load(counter);

describe("fixture:parallel_array_methods", () => {
  test("arr.map(fn) retorna vec com elementos transformados", () => {
    expect(gc.string_from_i64(dlen)).toBe("5");
    expect(gc.string_from_i64(d0)).toBe("2");
    expect(gc.string_from_i64(d4)).toBe("10");
  });

  test("arr.reduce(fn, init) acumula via parallel.reduce", () => {
    expect(gc.string_from_i64(sum)).toBe("15");
  });

  test("arr.forEach(fn) executa callback pra cada elemento", () => {
    expect(gc.string_from_i64(counterVal)).toBe("15");
  });
});
