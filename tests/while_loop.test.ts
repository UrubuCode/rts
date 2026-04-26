import { describe, test, expect } from "rts:test";
import { io, i32 } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

let i: i32 = 0;
let acc: i32 = 0;

while (i < 5) {
    acc = acc + i;
    i = i + 1;
}

print("acc:" + acc);

describe("fixture:while_loop", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("acc:10\n");
  });
});
