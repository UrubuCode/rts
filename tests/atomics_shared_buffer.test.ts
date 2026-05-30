import { describe, test, expect } from "rts:test";

let out: string = "";
function print(v: string): void { out += v + "\n"; }

// (#69) Atomics.* sobre Int32Array view de SharedArrayBuffer.
// RTS roda single-thread: cada op eh RMW nao-concorrente, semanticamente
// identica ao resultado observavel de Atomics em JS sem contencao.
const sab = new SharedArrayBuffer(16);
const a = new Int32Array(sab);
a[0] = 10;

const tSab: string = typeof SharedArrayBuffer;
const loadV = Atomics.load(a, 0);
const storeRet = Atomics.store(a, 1, 99);
const a1 = a[1];
const addPrev = Atomics.add(a, 0, 4);   // 10 -> 14
const afterAdd = a[0];
const subPrev = Atomics.sub(a, 0, 4);   // 14 -> 10
const afterSub = a[0];
const andPrev = Atomics.and(a, 0, 6);   // 10 & 6 = 2
const afterAnd = a[0];
const orPrev = Atomics.or(a, 0, 1);     // 2 | 1 = 3
const afterOr = a[0];
const xorPrev = Atomics.xor(a, 0, 3);   // 3 ^ 3 = 0
const afterXor = a[0];
const xchgPrev = Atomics.exchange(a, 0, 42); // store 42, prev 0
const afterXchg = a[0];
const casNoChg = Atomics.compareExchange(a, 0, 1, 7); // 42 != 1, no change, ret 42
const afterCasNoChg = a[0];
const casChg = Atomics.compareExchange(a, 0, 42, 7);  // 42 == 42, store 7, ret 42
const afterCasChg = a[0];

describe("atomics_shared_buffer (#69)", () => {
  test("typeof SharedArrayBuffer", () => expect(tSab).toBe("function"));
  test("load", () => expect(loadV).toBe(10));
  test("store retorna value", () => expect(storeRet).toBe(99));
  test("store grava", () => expect(a1).toBe(99));
  test("add retorna prev", () => expect(addPrev).toBe(10));
  test("add aplica", () => expect(afterAdd).toBe(14));
  test("sub retorna prev", () => expect(subPrev).toBe(14));
  test("sub aplica", () => expect(afterSub).toBe(10));
  test("and retorna prev", () => expect(andPrev).toBe(10));
  test("and aplica", () => expect(afterAnd).toBe(2));
  test("or aplica", () => expect(afterOr).toBe(3));
  test("xor aplica", () => expect(afterXor).toBe(0));
  test("exchange retorna prev", () => expect(xchgPrev).toBe(0));
  test("exchange aplica", () => expect(afterXchg).toBe(42));
  test("compareExchange sem troca retorna prev", () => expect(casNoChg).toBe(42));
  test("compareExchange sem troca mantem", () => expect(afterCasNoChg).toBe(42));
  test("compareExchange com troca retorna prev", () => expect(casChg).toBe(42));
  test("compareExchange com troca aplica", () => expect(afterCasChg).toBe(7));
});
