import { describe, test, expect } from "rts:test";

// (#103) JSON.stringify deve filtrar Symbol keys (encoded como
// `@@sym:<handle>` internamente pelo RTS apos #753). JS spec: Symbol
// keys nunca aparecem em JSON output.

const sym = Symbol("z");
const obj: any = {};
obj["a"] = 1;
obj[sym] = "secret";
obj["b"] = 2;

const json = JSON.stringify(obj);

const empty: any = {};
empty[Symbol("x")] = "v";
const emptyJson = JSON.stringify(empty);

describe("JSON.stringify filtra Symbol keys (#103)", () => {
  test("nao vaza @@sym:* em output normal", () =>
    expect(json).toBe('{"a":1,"b":2}'));
  test("obj so' com Symbol key -> {}", () => expect(emptyJson).toBe("{}"));
});
