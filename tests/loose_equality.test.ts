import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// === sem coercion: tipos diferentes -> false
print(`${0 === false}`);
print(`${1 === true}`);
print(`${"1" === 1}`);
print(`${0 === 0}`);
print(`${true === true}`);

// == com coercion: bool <-> number
print(`${0 == false}`);
print(`${1 == true}`);
print(`${0 == true}`);

// == com coercion: string <-> number
print(`${"1" == 1}`);
print(`${"42" == 42}`);
print(`${"x" == 1}`);

describe("loose_equality", () => {
  test("=== strict, == coerces", () =>
    expect(__rtsCapturedOutput).toBe(
      "false\nfalse\nfalse\ntrue\ntrue\ntrue\ntrue\nfalse\ntrue\ntrue\nfalse\n"
    ));
});
