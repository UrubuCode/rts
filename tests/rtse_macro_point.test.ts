import { describe, test, expect } from "rts:test";

// Proof of the #[rtse::class] authoring macro (RTS_ENGINE_ABI_CODEGEN): `Point`
// is authored as a normal Rust struct + impl in rts-shared; the macro generates
// the extern-C ABI glue. Used here as a JS class.

const p = new Point(3, 4);
const sum = p.sum();               // scalar return
const scaled = p.scaled(2);        // scalar param + return
const label = p.label();           // String return → string handle
const unit = Point.unit();         // static method (`statical`)
const tagged = p.tagged("val:");   // &str param (StrPtr) + String return

// &mut self — mutation persists in the handle (fresh instance).
const q = new Point(3, 4);
const bump1 = q.bump();            // x:3->4, 4+4=8
const bump2 = q.bump();            // x:4->5, 5+4=9
const qsum = q.sum();             // 9 (mutation persisted)

describe("rtse macro (Point)", () => {
    test("scalar method (sum)", () => { expect(sum).toBe(7); });
    test("scalar param + return (scaled)", () => { expect(scaled).toBe(14); });
    test("String return marshalled", () => { expect(label).toBe("(3,4)"); });
    test("static method (unit)", () => { expect(unit).toBe(1); });
    test("&str param + String return (tagged)", () => { expect(tagged).toBe("val:7"); });
    test("&mut self mutating method", () => { expect(bump1).toBe(8); });
    test("&mut self second call", () => { expect(bump2).toBe(9); });
    test("mutation persisted in handle", () => { expect(qsum).toBe(9); });
});
