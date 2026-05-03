import { describe, test, expect } from "rts:test";

let out = "";

const m = new Map<string, number>();
m.set("a", 1);
m.set("b", 2);
m.set("c", 3);

// .size
out += m.size + "\n";

// .keys() — armazenar em var pra evitar chained call ainda nao otimizado
const ks = m.keys();
out += ks.join(",") + "\n";

// .values()
const vs = m.values();
out += vs.join(",") + "\n";

// .entries() — Vec de Vec[k_handle, v]
const entries = m.entries();
out += entries.length + "\n";

// for-of destructure (#210 + #222)
for (const [k, v] of m.entries()) {
  out += k + "=" + v + "\n";
}

describe("map_iteration", () => {
  test("acceptance #222 v0", () => expect(out).toBe(
    "3\na,b,c\n1,2,3\n3\na=1\nb=2\nc=3\n"
  ));
});
