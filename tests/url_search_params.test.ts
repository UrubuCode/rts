import { describe, test, expect } from "rts:test";

let out = "";

// (#373) URLSearchParams minimal — get/has/set/delete/toString
const sp = new URLSearchParams("a=1&b=2&c=3");
out += sp.get("a") + "\n";   // 1
out += sp.get("b") + "\n";   // 2
out += sp.get("z") + "\n";   // null → "null" em concat (JS spec: null + "" = "null")

const hasA: boolean = sp.has("a");
const hasZ: boolean = sp.has("z");
out += (hasA ? "yes" : "no") + "\n";  // yes
out += (hasZ ? "yes" : "no") + "\n";  // no

sp.set("d", "4");
out += sp.get("d") + "\n";   // 4

sp.delete("a");
const hasA2: boolean = sp.has("a");
out += (hasA2 ? "yes" : "no") + "\n";  // no

out += sp.toString() + "\n";  // b=2&c=3&d=4

describe("url_search_params", () => {
  test("get/has/set/delete/toString (#373)", () => expect(out).toBe(
    "1\n2\nnull\nyes\nno\n4\nno\nb=2&c=3&d=4\n"
  ));
});
