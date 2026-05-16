import { describe, test, expect } from "rts:test";

// (#180/#294) fn.call/apply retornavam handle i64 raw que era
// renderizado como numero decimal (`281474976710670`) em template
// literal/concat. Fix: marca o resultado de call/apply como
// var_member_call_values para que TPL_COERCE_AUTO detecte handle
// de string em runtime.

function greet(this: any, hello: string, name: string) {
  return hello + " " + name;
}

const a = "out=" + greet.call(null, "Hi", "Alice");
const b = "out=" + greet.apply(null, ["Hello", "Bob"]);
const c = "lvl=" + greet.call(null, "Hey", "Carol");

describe("fn.call/apply retorna handle de string corretamente (#180)", () => {
  test("greet.call renderiza string", () => expect(a.indexOf("281474") < 0).toBe(true));
  test("greet.apply renderiza string", () => expect(b.indexOf("281474") < 0).toBe(true));
  test("greet.call retorna content nao-numerico", () =>
    expect(c.indexOf("281474") < 0).toBe(true));
});
