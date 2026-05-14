import { describe, test, expect } from "rts:test";

let out: string = "";
function print(v: string): void { out += v + "\n"; }

const u = new URIError("bad uri");
print("uri_name=" + u.name);
print("uri_msg=" + u.message);
print("uri_str=" + u.toString());

const e = new EvalError("eval err");
print("eval_name=" + e.name);
print("eval_msg=" + e.message);

// Coexistencia com outros tipos
const t = new TypeError("t");
print("type_name=" + t.name);

describe("URIError / EvalError (#794)", () => {
  test("matches expected stdout", () =>
    expect(out).toBe(
      "uri_name=URIError\n" +
      "uri_msg=bad uri\n" +
      "uri_str=URIError: bad uri\n" +
      "eval_name=EvalError\n" +
      "eval_msg=eval err\n" +
      "type_name=TypeError\n"
    )
  );
});
