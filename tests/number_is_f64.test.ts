import { describe, test, expect } from "rts:test";
import { io, gc } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// #323: `: number` deve mapear para F64 (alinhar com JS).
// Antes: divisao integer (sentinel 0) e perda de fracao.

const x: number = 1.0;
const y: number = 0.0;
const z: number = x / y;

if (z > 1e300) {
  const h = gc.string_from_static("Infinity");
  print(h); gc.string_free(h);
} else {
  const h = gc.string_from_static("not-infinity");
  print(h); gc.string_free(h);
}

const a: number = 7;
const b: number = 2;
const c: number = a / b;
if (c > 3.4 && c < 3.6) {
  const h = gc.string_from_static("3.5");
  print(h); gc.string_free(h);
} else {
  const h = gc.string_from_static("wrong");
  print(h); gc.string_free(h);
}

let acc: number = 0;
acc = acc + 1.5;
acc = acc + 2.5;
if (acc > 3.9 && acc < 4.1) {
  const h = gc.string_from_static("4");
  print(h); gc.string_free(h);
}

describe("fixture:number_is_f64", () => {
  test("number type is f64 with correct semantics", () => {
    expect(__rtsCapturedOutput).toBe("Infinity\n3.5\n4\n");
  });
});
