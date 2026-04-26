import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

function main() {
  let a = 1;
  {
    let a = 2;
    print(`inner: ${a}`);
  }
  print(`outer: ${a}`);

  let b = 10;
  b = 20;
  print(`b = ${b}`);

  const c = 3;
  print(`c = ${c}`);

  {
    var v = 7;
  }
  print(`v = ${v}`);
}

main();

describe("fixture:let_const_var", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("inner: 2\nouter: 1\nb = 20\nc = 3\nv = 7\n");
  });
});
