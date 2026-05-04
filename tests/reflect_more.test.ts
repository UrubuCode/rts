import { describe, test, expect } from "rts:test";

let out = "";

// (#218) getPrototypeOf
const proto = { greet: 0 };
const child = Object.create(proto);
const got = Reflect.getPrototypeOf(child);
out += (got !== 0 ? "has-proto" : "no-proto") + "\n";  // has-proto

// setPrototypeOf
const obj: { [k: string]: number } = {};
const newProto = { kind: 0 };
const setOk: boolean = Reflect.setPrototypeOf(obj, newProto);
out += (setOk ? "set-ok" : "set-fail") + "\n";        // set-ok
const newGot = Reflect.getPrototypeOf(obj);
out += (newGot !== 0 ? "linked" : "no-link") + "\n";  // linked

// isExtensible — v0 sempre true
const ext: boolean = Reflect.isExtensible(obj);
out += (ext ? "y" : "n") + "\n";   // y

// preventExtensions — v0 no-op, retorna true
const pe: boolean = Reflect.preventExtensions(obj);
out += (pe ? "y" : "n") + "\n";    // y

describe("reflect_more", () => {
  test("getPrototypeOf/setPrototypeOf/isExtensible/preventExtensions (#218)", () => expect(out).toBe(
    "has-proto\nset-ok\nlinked\ny\ny\n"
  ));
});
