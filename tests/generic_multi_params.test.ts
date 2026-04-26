import { describe, test, expect } from "rts:test";
import { io, gc } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Multi type params <K, V>.

function makePair<K, V>(k: K, v: V): K {
  return k;
}

function takeSecond<K, V>(k: K, v: V): V {
  return v;
}

const a = makePair<i64, i64>(99, 200);
const b = takeSecond<i64, i64>(99, 200);

const h1 = gc.string_from_i64(a);
print(h1); gc.string_free(h1);
const h2 = gc.string_from_i64(b);
print(h2); gc.string_free(h2);

describe("fixture:generic_multi_params", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("99\n200\n");
  });
});
