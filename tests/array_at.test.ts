import { describe, test, expect } from "rts:test";

let out: string = "";
function print(v: string): void { out += v + "\n"; }

const values = [5, 12, 8, 130, 44];
print("at_neg2=" + values.at(-2));
print("at_0=" + values.at(0));
print("at_last=" + values.at(4));
print("at_neg1=" + values.at(-1));
print("at_oor_pos=" + values.at(99));
print("at_oor_neg=" + values.at(-99));

// Em loop / condicional
let acc = 0;
for (let i = 0; i < values.length; i++) {
  acc += values.at(i) as number;
}
print("sum=" + acc);

// Array de strings — ainda deve formatar corretamente
const tags = ["a", "b", "c"];
print("tag_neg=" + tags.at(-1));
print("tag_0=" + tags.at(0));

describe("array.at — negative + OOR", () => {
  test("matches expected stdout", () =>
    expect(out).toBe(
      "at_neg2=130\n" +
      "at_0=5\n" +
      "at_last=44\n" +
      "at_neg1=44\n" +
      "at_oor_pos=undefined\n" +
      "at_oor_neg=undefined\n" +
      "sum=199\n" +
      "tag_neg=c\n" +
      "tag_0=a\n"
    )
  );
});
