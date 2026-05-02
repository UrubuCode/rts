import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// 1. Sem interpolacao
print(`hello world`);

// 2. Interpolacao basica de string/i32/f64
const name = "RTS";
print(`hello ${name}`);

const n = 42;
print(`answer is ${n}`);

const pi = 3.14;
print(`pi = ${pi}`);

// 3. Multiplas interpolacoes na mesma string
const x = 10;
const y = 20;
print(`${x} + ${y} = ${x + y}`);

// 4. Caracteres ao redor da interpolacao (regressao: literal NAO some)
print(`[${name}]`);
print(`<<${n}>>`);
print(`${name}!${n}!${pi}!`);

// 5. Concat com template
const greet = `hi ` + name;
print(greet);
print(`pre-${greet}-post`);

// 6. Expressoes nao-triviais dentro de ${}
print(`sum=${1 + 2 + 3}`);
print(`cond=${x > y ? "x>y" : "x<=y"}`);
function double(v: number): number { return v * 2; }
print(`call=${double(7)}`);

// 7. Boolean e undefined em interpolacao
print(`b=${true} f=${false} u=${undefined}`);

// 8. Escape sequences dentro de template (tab, backslash)
print(`tab=\t end`);
print(`bs=\\ end`);

// 9. String vazia interpolada (regressao: nao come o delimitador)
const empty: string = "";
print(`[${empty}][${empty}]`);

// 10. Var no comeco/fim sem texto literal adjacente
print(`${name}`);
print(`${n}`);

describe("template literals — interpolacao, escape, vazia, expressoes", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe(
      "hello world\n" +
      "hello RTS\n" +
      "answer is 42\n" +
      "pi = 3.14\n" +
      "10 + 20 = 30\n" +
      "[RTS]\n" +
      "<<42>>\n" +
      "RTS!42!3.14!\n" +
      "hi RTS\n" +
      "pre-hi RTS-post\n" +
      "sum=6\n" +
      "cond=x<=y\n" +
      "call=14\n" +
      "b=true f=false u=undefined\n" +
      "tab=\t end\n" +
      "bs=\\ end\n" +
      "[][]\n" +
      "RTS\n" +
      "42\n"
    );
  });
});
