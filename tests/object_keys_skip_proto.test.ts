import { describe, test, expect } from "rts:test";

let out = "";

// (#208 fix) Object.keys/values/entries nao retornam __proto__.
const proto = { greet: 0 };
const child = Object.create(proto);

const keys = Object.keys(child);
out += keys.length + "\n";   // 0 (sem own)

const vals = Object.values(child);
out += vals.length + "\n";   // 0

const entries = Object.entries(child);
out += entries.length + "\n";// 0

// Com own props
const obj: { [k: string]: number } = Object.create(proto);
obj.a = 1;
obj.b = 2;
const k2 = Object.keys(obj);
out += k2.length + "\n";     // 2 (so' a, b — sem __proto__)

describe("object_keys_skip_proto", () => {
  test("keys/values/entries excluem __proto__ (#208)", () => expect(out).toBe(
    "0\n0\n0\n2\n"
  ));
});
