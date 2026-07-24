import { describe, test, expect } from "rts:test";

// Proof of the #[rtse::class] authoring macro (RTS_ENGINE_ABI_CODEGEN): `Point`
// is authored as a normal Rust struct + impl in rts-shared; the macro generates
// the extern-C ABI glue (methods on the impl, field accessors on the struct).

const p = new RtsePoint(3, 4);
const sum = p.sum();
const scaled = p.scaled(2);
const label = p.label();
const unit = RtsePoint.unit();
const tagged = p.tagged("val:");

const q = new RtsePoint(3, 4);
const bump1 = q.bump();
const bump2 = q.bump();
const qsum = q.sum();

// #[rtse::variable] field accessors.
const r = new RtsePoint(3, 4);
const rx = r.x;           // getter → 3
const ry = r.y;           // readonly getter → 4
r.x = 10;                 // setter
const rx2 = r.x;          // 10
const rsum = r.sum();     // 10+4=14

describe("rtse macro (Point)", () => {
    test("scalar method (sum)", () => { expect(sum).toBe(7); });
    test("scalar param + return (scaled)", () => { expect(scaled).toBe(14); });
    test("String return marshalled", () => { expect(label).toBe("(3,4)"); });
    test("static method (unit)", () => { expect(unit).toBe(1); });
    test("&str param + String return (tagged)", () => { expect(tagged).toBe("val:7"); });
    test("&mut self mutating method", () => { expect(bump1).toBe(8); });
    test("&mut self second call", () => { expect(bump2).toBe(9); });
    test("mutation persisted in handle", () => { expect(qsum).toBe(9); });
    test("variable getter (x)", () => { expect(rx).toBe(3); });
    test("readonly variable getter (y)", () => { expect(ry).toBe(4); });
    test("variable setter (x)", () => { expect(rx2).toBe(10); });
    test("setter reflected in method", () => { expect(rsum).toBe(14); });
});
