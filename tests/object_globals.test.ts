import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// (#266) Object.keys, Object.values, Object.hasOwn — globais JS sobre
// object literals (que sao Map handle no RTS).

const obj = { a: 1, b: 2, c: 3 };

// 1. Object.keys
const ks = Object.keys(obj);
print(ks.join(","));   // a,b,c (sorted)

// 2. Object.values
const vs = Object.values(obj);
print(vs.join(","));   // 1,2,3 (in key-sorted order)

// 3. Object.hasOwn
print(`${Object.hasOwn(obj, "a")}`);   // true
print(`${Object.hasOwn(obj, "z")}`);   // false

// 4. Object.keys vazio
const empty = {};
print(`empty.length=${Object.keys(empty).length}`);  // 0

// 5. Iterar via Object.keys + lookup — pattern comum
const config = { host: "localhost", port: 8080, debug: 1 };
const keys = Object.keys(config);
print(`config has ${keys.length} keys`);  // 3

describe("object_globals", () => {
  test("Object.keys/values/hasOwn", () =>
    expect(__rtsCapturedOutput).toBe(
      "a,b,c\n" +
      "1,2,3\n" +
      "true\nfalse\n" +
      "empty.length=0\n" +
      "config has 3 keys\n"
    ));
});
