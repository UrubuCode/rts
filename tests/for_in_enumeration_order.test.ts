import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// JS: integer-indexed keys ascendentes, depois string keys em ordem de inserção.
const obj: any = {};
obj["b"] = 1;
obj["2"] = 2;
obj["a"] = 3;
obj["0"] = 4;
obj["1"] = 5;
obj["c"] = 6;

let order = "";
for (const k in obj) {
  order += k + ",";
}
print(order);

describe("for_in_enumeration_order", () => {
  test("integers first ascending then strings in insertion order", () => {
    expect(__rtsCapturedOutput).toBe("0,1,2,b,a,c,\n");
  });
});
