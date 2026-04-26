import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

function main() {
  let x: i32 = 10;
  x += 5;
  print(`${x}`);
  x -= 3;
  print(`${x}`);
  x *= 2;
  print(`${x}`);
  x /= 4;
  print(`${x}`);
  x %= 4;
  print(`${x}`);

  let bits: i32 = 0xF0;
  bits &= 0x3C;
  print(`${bits}`);
  bits |= 0x03;
  print(`${bits}`);
  bits ^= 0x33;
  print(`${bits}`);

  let shift: i32 = 1;
  shift <<= 4;
  print(`${shift}`);
  shift >>= 2;
  print(`${shift}`);
}

main();

describe("fixture:compound_assign", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("15\n12\n24\n6\n2\n48\n51\n0\n16\n4\n");
  });
});
