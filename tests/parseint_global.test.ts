import { describe, test, expect } from "rts:test";

let out = "";

// (#208) parseInt/parseFloat globais (sem trailing chars).
out += parseInt("100") + "\n";
out += parseInt("-42") + "\n";
out += parseFloat("3.14") + "\n";
out += parseFloat("2.5") + "\n";

// Number.parseInt/parseFloat (já existiam)
out += Number.parseInt("99") + "\n";
out += Number.parseFloat("1.5") + "\n";

describe("parseint_global", () => {
  test("parseInt/parseFloat globais (#208)", () => expect(out).toBe(
    "100\n-42\n3.14\n2.5\n99\n1.5\n"
  ));
});
