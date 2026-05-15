import { describe, test, expect } from "rts:test";

let out: string = "";
function print(v: string): void { out += v + "\n"; }

const errs = [new Error("a"), new Error("b")];
const agg = new AggregateError(errs, "multiple");

print("name=" + agg.name);
print("msg=" + agg.message);
print("len=" + agg.errors.length);

// Mensagem opcional ausente
const agg2 = new AggregateError([new Error("only")]);
print("name2=" + agg2.name);
print("len2=" + agg2.errors.length);

describe("AggregateError (#748)", () => {
  test("matches expected stdout", () =>
    expect(out).toBe(
      "name=AggregateError\n" +
      "msg=multiple\n" +
      "len=2\n" +
      "name2=AggregateError\n" +
      "len2=1\n"
    )
  );
});
