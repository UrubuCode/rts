import { describe, test, expect } from "rts:test";

// (#573) console.log com Handle ambiguo (WeakMap.get retorna i64
// disfarcado de Handle) deve formatar como numero quando handle
// nao e' string valida.

let __out: string = "";
function p(v: string): void { __out += v + "\n"; }
function p2(v: number): void { __out += v + "\n"; }

const wm = new WeakMap();
const k = {};
wm.set(k, 99);
const got = wm.get(k);
p2(got);

describe("console_log_handle_auto_coerce", () => {
  test("WeakMap.get(k) retorna 99 (#573)", () => expect(got).toBe(99));
});
