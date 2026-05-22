// Cross-runtime: XOR string encoding/decoding (js-confuser cipher pattern).
function _xorDecode(encoded: number[], key: number): string {
  return encoded.map(c => String.fromCharCode(c ^ key)).join("");
}

// "hello" XOR'd with key 0x42
const _e1 = [0x2a, 0x27, 0x2e, 0x2e, 0x2d];
console.log(_xorDecode(_e1, 0x42));

// "world" XOR'd with key 0x15
const _e2 = [0x62, 0x6a, 0x7f, 0x79, 0x69];
console.log(_xorDecode(_e2, 0x15));

// multi-key XOR (rolling key — common in stronger obfuscation)
function _rollingXor(encoded: number[], keys: number[]): string {
  return encoded.map((c, i) => String.fromCharCode(c ^ keys[i % keys.length])).join("");
}
// "obfuscated" with key [0x11, 0x22, 0x33]
const _e3 = [0x7e, 0x46, 0x57, 0x74, 0x47, 0x56, 0x50, 0x43, 0x57, 0x75];
console.log(_rollingXor(_e3, [0x11, 0x22, 0x33]));

// charcode array → string (direct obfuscator pattern, no XOR)
const _raw = [74, 97, 118, 97, 83, 99, 114, 105, 112, 116];
console.log(_raw.map(c => String.fromCharCode(c)).join(""));

// hex escape string decode
const _hex = "\x68\x65\x6c\x6c\x6f\x20\x52\x54\x53";
console.log(_hex);

// unicode escape
const _uni = "RTS runs";
console.log(_uni);
