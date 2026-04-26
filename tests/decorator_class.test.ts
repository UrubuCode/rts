import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Class decorator: executa como side-effect na declaracao.

function register(target: i64): i64 {
  print("classe registrada");
  return target;
}

@register
class Service {
  greet(): void {
    print("oi do service");
  }
}

new Service().greet();

describe("fixture:decorator_class", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("classe registrada\noi do service\n");
  });
});
