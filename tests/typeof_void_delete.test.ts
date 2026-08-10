import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

function main() {
  const n: i32 = 42;
  const s: string = "hello";
  const b: boolean = true;

  print(typeof n);
  print(typeof s);
  print(typeof b);

  // void avalia e descarta — o retorno (0) nao deve surgir no fluxo
  // alem do que atribuirmos.
  const voided = void 999;
  print(`voided = ${voided}`);

  // `delete n` sobre uma ligacao lexica estava aqui a afirmar `true`, que o
  // JavaScript nao produz em lado nenhum: em sloppy mode responde `false`
  // (verificado no Node), e num MODULO — que e como este motor compila tudo — e
  // um SyntaxError precoce, portanto o ficheiro nem era um modulo legal.
  //
  // O caso que resta e o unico `delete` que a linguagem define aqui: sobre uma
  // PROPRIEDADE. O `delete` de ligacao fica de fora ate haver quem recuse um
  // programa por erro precoce, que e trabalho de uma fase de verificacao e nao
  // um valor que o emissor possa inventar.
  const holder = { gone: 1 };
  const d = delete holder.gone;
  print(`delete = ${d} ${holder.gone}`);
}

main();

describe("fixture:typeof_void_delete", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("number\nstring\nboolean\nvoided = undefined\ndelete = true undefined\n");
  });
});
