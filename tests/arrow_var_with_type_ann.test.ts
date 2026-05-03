import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// (#455) Antes: arrow chamada via var local com return type number retornava 0
// porque a annotation `: () => number` na binding nao propagava para o
// arrow lifted. Agora propaga.

// 1. Annotation simples `() => number`
const f1: () => number = () => 42;
print("a=" + f1());

// 2. Com param: `(x: number) => number`
const f2: (x: number) => number = (x) => x * 2;
print("b=" + f2(7));

// 3. Union com null: `(() => number) | null`
const f3: (() => number) | null = () => 99;
print("c=" + (f3 === null ? -1 : f3()));

// 4. Union dentro de paren: `((() => number)) | undefined`
const f4: (() => number) | undefined = () => 100;
print("d=" + (f4 === undefined ? -1 : f4()));

// 5. Arrow com annotation propria continua funcionando
const f5: () => number = (): number => 7;
print("e=" + f5());

describe("arrow via var com type ann (#455)", () => {
  test("propaga return type da binding pro arrow lifted", () =>
    expect(__rtsCapturedOutput).toBe(
      "a=42\n" +
      "b=14\n" +
      "c=99\n" +
      "d=100\n" +
      "e=7\n"
    ));
});
