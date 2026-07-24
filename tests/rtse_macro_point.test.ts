import { describe, test, expect } from "rts:test";

// Proof of the #[rtse::class] authoring macro (RTS_ENGINE_ABI_CODEGEN): the
// `Point` class is authored as a normal Rust struct + impl in rts-shared; the
// macro generates the extern-C ABI glue. Here it is used as a JS class.

const p = new Point(3, 4);
const sum = p.sum();               // f64 return
const scaled = p.scaled(2);        // f64 param + return
const label = p.label();           // String return → string handle

describe("rtse macro (Point)", () => {
    test("scalar method (sum)", () => { expect(sum).toBe(7); });
    test("scalar param + return (scaled)", () => { expect(scaled).toBe(14); });
    test("String return marshalled to string", () => { expect(label).toBe("(3,4)"); });
});
