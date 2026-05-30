import { describe, test, expect } from "rts:test";

let out: string = "";
function print(v: string): void { out += v + "\n"; }

// (#68) structuredClone(buf, { transfer: [buf] }) — buffer fonte detached
// (byteLength 0), clone independente com os bytes. Cobre: anonymous view
// set/read (`new Uint8Array(buf).set(...)` / `Array.from(new Uint8Array)`)
// + structuredClone byte-copy + detach.
const buffer = new ArrayBuffer(4);
new Uint8Array(buffer).set([10, 20, 30, 40]);
const moved = structuredClone(buffer, { transfer: [buffer] });

const origLen = buffer.byteLength;       // 0 (detached)
const movedLen = moved.byteLength;       // 4
const movedBytes = Array.from(new Uint8Array(moved)).join(","); // 10,20,30,40

// clone SEM transfer nao detacha.
const keep = new ArrayBuffer(2);
new Uint8Array(keep).set([1, 2]);
const copy = structuredClone(keep);
const keepLen = keep.byteLength;         // 2 (intacto)
const copyBytes = Array.from(new Uint8Array(copy)).join(","); // 1,2

describe("arraybuffer_transfer_clone (#68)", () => {
  test("buffer fonte detached (byteLength 0)", () => expect(origLen).toBe(0));
  test("clone preserva byteLength", () => expect(movedLen).toBe(4));
  test("clone preserva bytes", () => expect(movedBytes).toBe("10,20,30,40"));
  test("clone sem transfer nao detacha", () => expect(keepLen).toBe(2));
  test("clone sem transfer copia bytes", () => expect(copyBytes).toBe("1,2"));
});
