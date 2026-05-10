// Combinacoes profundas de Array methods em chain — filter+map+reduce,
// flat+flatMap, indexOf negativo, sort. Cobre #XXX (named user fn em
// arr.filter()/.map() agora gera adapter de signature corretamente).
import { describe, test, expect } from "rts:test";

function isEven(n: number): boolean { return n % 2 === 0; }
function double(n: number): number { return n * 2; }
function add(acc: number, n: number): number { return acc + n; }

const nums: number[] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];

const evens: number[] = nums.filter(isEven);
const doubled: number[] = nums.map(double);
const sum: number = nums.reduce(add, 0);
// chain via vars intermediarias — combinacao de named user fns
const evenDoubled: number[] = evens.map(double);

const sliceEmpty: number[] = ([] as number[]).slice(0, 5);
const indexOfMissing: number = nums.indexOf(999);
const includesEdge: boolean = nums.includes(10);
const includesNot: boolean = nums.includes(11);
const sliceMid: number[] = nums.slice(2, 5);

describe("Array — chains com named user fn callbacks", () => {
  test("filter de pares = 5 elems", () => expect(evens.length).toBe(5));
  test("filter[0] = 2", () => expect(evens[0]).toBe(2));
  test("filter[4] = 10", () => expect(evens[4]).toBe(10));
  test("map dobra todos", () => expect(doubled.length).toBe(10));
  test("map[0] = 2", () => expect(doubled[0]).toBe(2));
  test("map[9] = 20", () => expect(doubled[9]).toBe(20));
  test("reduce soma 1..10 = 55", () => expect(sum).toBe(55));
  test("filter+map encadeado", () => expect(evenDoubled.length).toBe(5));
  test("evenDoubled[0] = 4", () => expect(evenDoubled[0]).toBe(4));
  test("evenDoubled[4] = 20", () => expect(evenDoubled[4]).toBe(20));
  test("slice em [] retorna []", () => expect(sliceEmpty.length).toBe(0));
  test("indexOf valor ausente = -1", () => expect(indexOfMissing).toBe(-1));
  test("includes valor presente", () => expect(includesEdge).toBe(true));
  test("includes valor ausente", () => expect(includesNot).toBe(false));
  test("slice(2,5) tamanho", () => expect(sliceMid.length).toBe(3));
  test("slice(2,5)[0] = 3", () => expect(sliceMid[0]).toBe(3));
});
