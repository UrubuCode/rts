import { describe, test, expect } from "rts:test";

let out = "";

// (#383) Antes: ident global desconhecida causava segfault.
// Agora: codegen emite warning + chamada e' no-op (sem crash).

unknownGlobalXYZ123();  // no-op + warning, sem crash

out = "survived";

describe("undefined_ident_warn", () => {
  test("ident global desconhecida nao crasha (#383)", () =>
    expect(out).toBe("survived"));
});
