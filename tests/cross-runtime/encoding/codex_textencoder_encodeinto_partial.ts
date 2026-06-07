// Cross-runtime: TextEncoder.encodeInto partial writes.
const enc = new TextEncoder();
const dest = new Uint8Array(5);
const r = enc.encodeInto("A🙂B", dest);
console.log(r.read + ":" + r.written);
console.log(Array.from(dest).join(","));
console.log(new TextDecoder().decode(dest.slice(0, r.written)));
