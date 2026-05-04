import { describe, test, expect } from "rts:test";

let out = "";

const sp = new URLSearchParams("a=1&b=2");

// keys()/values() — Vec
const ks = sp.keys();
out += ks.join(",") + "\n";    // a,b

const vs = sp.values();
out += vs.join(",") + "\n";    // 1,2

// append (v0 = set)
sp.append("c", "3");
out += sp.get("c") + "\n";     // 3

// getAll — v0 retorna Vec[1] ou Vec[0]
const all = sp.getAll("a");
out += all.length + "\n";      // 1
out += all.join(",") + "\n";   // 1

const empty = sp.getAll("z");
out += empty.length + "\n";    // 0

describe("url_search_params_more", () => {
  test("append/getAll/keys/values (#373 v0)", () => expect(out).toBe(
    "a,b\n1,2\n3\n1\n1\n0\n"
  ));
});
