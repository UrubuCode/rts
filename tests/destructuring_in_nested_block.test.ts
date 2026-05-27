import { describe, test, expect } from "rts:test";

// (cross-runtime #368) destructuring decl (`const {a,b} = e` / `const [a,b]=e`)
// dentro de bloco aninhado (for/while/if/try-catch). Antes do fix, o pass
// expand_destructuring so' olhava statements diretos (top-level e corpo de fn
// no 1o nivel), nunca descia em blocos aninhados. Resultado: destructuring
// dentro de loop/if dava "destructuring not supported" (em fn) ou
// "invalid block reference" (no top-level).

function arrayDestructInLoop(): string {
  const out: number[] = [];
  for (let i = 0; i < 3; i++) {
    const [a, b] = [i * 10, i * 100];
    out.push(a + b);
  }
  return out.join(",");
}

function objectDestructInIf(): number {
  let sum = 0;
  if (1 < 2) {
    const { x, y } = { x: 3, y: 4 };
    sum = x + y;
  }
  return sum;
}

function objectDestructInWhile(): string {
  const obj = { _i: 0, next() { this._i++; return { value: this._i }; } };
  const out: number[] = [];
  let k = 0;
  while (k < 3) {
    const { value } = obj.next();
    out.push(value);
    k++;
  }
  return out.join(",");
}

function destructInCatch(): number {
  try {
    throw new Error("boom");
  } catch (e: any) {
    const [a, b] = [5, 6];
    return a + b;
  }
}

const a = arrayDestructInLoop();
const b = objectDestructInIf();
const c = objectDestructInWhile();
const d = destructInCatch();

describe("destructuring in nested block (#368)", () => {
  test("array destruct in for", () => expect(a).toBe("0,110,220"));
  test("object destruct in if", () => expect(`${b}`).toBe("7"));
  test("object destruct in while", () => expect(c).toBe("1,2,3"));
  test("destruct in catch", () => expect(`${d}`).toBe("11"));
});
