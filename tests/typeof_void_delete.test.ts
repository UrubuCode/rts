import { describe, test, expect } from "rts:test";
import { io } from "rts";

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

  // delete em variavel non-property e no-op; retorna true.
  const d = delete n;
  print(`delete = ${d}`);
}

main();

describe("fixture:typeof_void_delete", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("number\nstring\nboolean\nvoided = 0\ndelete = 1\n");
  });
});
