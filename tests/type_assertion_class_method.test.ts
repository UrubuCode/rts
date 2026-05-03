import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// (#fix) `(c as Counter).bump()` antes crashava com SIGILL porque o
// fast path do PR5 (#264) sequestrava todos TsAs.method() para
// var_member_call generico, ignorando que a annotation resolvia
// para classe registrada.

class Counter {
  n: number = 0;
  bump(): number {
    this.n = this.n + 1;
    return this.n;
  }
  reset(): void {
    this.n = 0;
  }
}

const c = new Counter();
const v1: number = (c as Counter).bump();
const v2: number = (c as Counter).bump();
const v3: number = ((c as Counter) as Counter).bump();
print("v1=" + v1);
print("v2=" + v2);
print("v3=" + v3);

(c as Counter).reset();
const v4: number = (c as Counter).bump();
print("v4=" + v4);

describe("(receiver as Class).method() (#fix)", () => {
  test("class method routing preservado com TsAs", () =>
    expect(__rtsCapturedOutput).toBe(
      "v1=1\n" +
      "v2=2\n" +
      "v3=3\n" +
      "v4=1\n"
    ));
});
