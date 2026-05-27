import { describe, test, expect } from "rts:test";

// Regression (cross-runtime #54): `arr.filter(f).map(g)` encadeado DIRETO
// (sem var intermediaria) crashava com ILLEGAL_INSTRUCTION. Causa: o
// array_methods_pass reescrevia o `.map` externo para `parallel.map(arr, g)`
// e retornava cedo SEM reescrever o `.filter` interno (clonado como arr_expr)
// — o `.filter` ficava sem rewrite e o codegen fazia MAP_GET("filter") em
// Vec -> trapz -> SIGILL. Fix: recursar em arr_expr antes de montar o arg.

let out = "";
function print(v: string): void { out += v + "\n"; }

// chain com arrows
const words = ["hi", "world", "ab", "hello"];
print(words.filter(w => w.length > 2).map(w => w.toUpperCase()).join(","));  // WORLD,HELLO

// chain com user fns nomeadas
function even(n: number): boolean { return n % 2 === 0; }
function sq(n: number): number { return n * n; }
const nums = [1, 2, 3, 4, 5, 6];
print(nums.filter(even).map(sq).join(","));   // 4,16,36

// chain de 3: filter -> map -> filter
print(nums.filter(even).map(sq).filter(x => x > 10).join(","));  // 16,36

// var intermediaria continua OK
const f = words.filter(w => w.length > 2);
print(f.map(w => w.length).join(","));  // 5,5

describe("array filter map chain", () => {
  test("chain direto nao crasha", () =>
    expect(out).toBe("WORLD,HELLO\n4,16,36\n16,36\n5,5\n"));
});
