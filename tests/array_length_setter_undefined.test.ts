// (#861/214) arr.length = N (N > length) extende com sentinel undefined,
// nao com handle de string "undefined". Permite arr[i] === undefined ser true.
import { describe, test, expect } from "rts:test";

let out = "";
const arr = [1, 2, 3];
arr.length = 5;
out += "len=" + arr.length + "\n";
out += "extended=" + (arr[3] === undefined) + "\n";
out += "extended2=" + (arr[4] === undefined) + "\n";

arr.length = 2;
out += "truncated=" + arr.join(",") + "\n";
out += "trunc_len=" + arr.length + "\n";

describe("Array length setter undefined sentinel (#861)", () => {
  test("extend fills with undefined sentinel", () => expect(out).toBe(
    "len=5\nextended=true\nextended2=true\ntruncated=1,2\ntrunc_len=2\n"
  ));
});
