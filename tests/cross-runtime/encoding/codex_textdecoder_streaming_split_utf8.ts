// Cross-runtime: TextDecoder streaming across split UTF-8 code points.
const dec = new TextDecoder();
const first = dec.decode(new Uint8Array([0xf0, 0x9f]), { stream: true });
const second = dec.decode(new Uint8Array([0x99, 0x82]), { stream: true });
const third = dec.decode();
console.log(first.length + ":" + first);
console.log(second.length + ":" + second);
console.log(third.length + ":" + third);
