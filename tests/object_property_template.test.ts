import { describe, test, expect } from "rts:test";

let out = "";

// (#434) object literal property em template literal — antes retornava vazio.
const cfg = { port: 3000 } satisfies { port: number };
out += `${cfg.port}` + "\n";   // 3000

const config = { name: "rts", version: 1 };
out += `${config.name} v${config.version}` + "\n";

describe("object_property_template", () => {
  test("propriedade em template literal (#434)", () => expect(out).toBe(
    "3000\nrts v1\n"
  ));
});
