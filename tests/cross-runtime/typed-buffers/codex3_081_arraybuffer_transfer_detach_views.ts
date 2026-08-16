// Cross-runtime: transferring an ArrayBuffer detaches every view on the source buffer.
const buffer = new ArrayBuffer(8);
const bytes = new Uint8Array(buffer);
bytes.set([1, 2, 3, 4]);
const view = new DataView(buffer);
const clone = structuredClone(buffer, { transfer: [buffer] });
console.log(buffer.byteLength, clone.byteLength, new Uint8Array(clone).slice(0, 4).join(","));
const checks: boolean[] = [];
try { view.getUint8(0); } catch (e) { checks.push(e instanceof TypeError); }
try { bytes.slice(); } catch (e) { checks.push(e instanceof TypeError); }
console.log(checks.join(","));

