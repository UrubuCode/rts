import { describe, test, expect } from "rts:test";
import { gc } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

function add(a: i64, b: i64): i64 { return a + b; }
function multiply(a: i64, b: i64): i64 { return a * b; }
function noop(): i64 { return 100; }

// 1. fn.call(thisArg, ...args)
print("call=" + add.call(0, 3, 4));

// 2. fn.apply(thisArg, argsArray)
const arr = [10, 20];
print("apply=" + add.apply(0, arr));

// 3. fn.toString() — fn estatica retorna [native code]
const s = noop.toString();
print(s); gc.string_free(s);

// 4. new Function — compila body em runtime via eval
const sq = new Function("x", "return x * x;");
print("new=" + sq.call(0, 9));

// 5. fn.bind — partial application
const dbl = multiply.bind(0, 2);
print("bind=" + dbl.call(0, 7));

describe("Function global (#359)", () => {
  test("call/apply/toString/new/bind", () => {
    expect(__rtsCapturedOutput).toBe(
      "call=7\napply=30\nfunction noop() { [native code] }\nnew=81\nbind=14\n"
    );
  });
});
