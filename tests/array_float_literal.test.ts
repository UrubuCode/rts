import { describe, test, expect } from "rts:test";

// Regression (cross-runtime #1275): float fracionario literal em array era
// TRUNCADO (`[1.5,2.5][0]` -> 1, join -> 1,2). Fix: literal float fracionario
// armazena bits-f64 (bitcast) em vez de fcvt_to_sint; leitura (index/join/
// template/inspect) reinterpreta via heuristica >2^53 -> from_bits. F64
// inteiro-valued (i*10, 3.0) e int continuam i64 (destructuring/aritmetica OK).

let out = "";
function print(v: string): void { out += v + "\n"; }

// acesso por indice
print([1.5, 2.5, 3.0][0] + "");          // 1.5
print([1.5, 2.5, 3.0][1] + "");          // 2.5

// join
print([9.99, 19.99, 4.5].join(","));     // 9.99,19.99,4.5
print([1.5, 2.5, 3.0].join(","));        // 1.5,2.5,3 (3.0 -> "3")

// negativo fracionario
print([-3.5, 1.25].join(","));           // -3.5,1.25

// int continua int (guard)
print([1, 2, 3].join(","));              // 1,2,3
print([-5, 0, 1000000].join(","));       // -5,0,1000000

// F64 inteiro-valued via expr nao quebra destructuring (guard #368)
function destr(): string {
  const o: number[] = [];
  for (let i = 0; i < 3; i++) {
    const [a, b] = [i * 10, i * 100];
    o.push(a + b);
  }
  return o.join(",");
}
print(destr());                          // 0,110,220

describe("array float literal (#1275)", () => {
  test("float fracionario literal preserva valor; int/destruct intactos", () =>
    expect(out).toBe("1.5\n2.5\n9.99,19.99,4.5\n1.5,2.5,3\n-3.5,1.25\n1,2,3\n-5,0,1000000\n0,110,220\n"));
});
