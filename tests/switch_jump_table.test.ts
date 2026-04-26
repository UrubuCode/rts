import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

function label(n: i32): void {
  switch (n) {
    case 0: print("zero"); break;
    case 1: print("one"); break;
    case 2: print("two"); break;
    case 5: print("five"); break;
    case 10: print("ten"); break;
    default: print("other");
  }
}

label(0);
label(1);
label(2);
label(3);
label(5);
label(10);
label(99);
label(-1);

describe("fixture:switch_jump_table", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("zero\none\ntwo\nother\nfive\nten\nother\nother\n");
  });
});
