import { describe, test, expect } from "rts:test";

// (#316) `Set.values()` retornava [1, 1] em vez de [2, 3] porque o
// dispatch caia em MAP_VALUES que retorna `m.values()` do storage
// interno (Map<elemento_str, 1>). JS spec: Set.values() retorna os
// elementos (= keys do storage). Fix: MAP_VALUES detecta Set via
// handle_is_set_kind e retorna keys parseadas (parse_array_index ou
// string handle).

const s = new Set([2, 3, 5]);
const v = Array.from(s.values()).join(",");

const sStr = new Set(["a", "b"]);
const vStr = Array.from(sStr.values()).join(",");

describe("Set.values retorna elementos, nao internal values (#316)", () => {
  test("Set numerico", () => expect(v).toBe("2,3,5"));
  test("Set string", () => expect(vStr).toBe("a,b"));
});
