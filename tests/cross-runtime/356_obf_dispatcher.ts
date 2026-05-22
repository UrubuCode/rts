// Cross-runtime: opcode dispatcher pattern (js-confuser function outlining).
// Functions decomposed into opcodes dispatched via lookup table.

type Op = (...args: number[]) => number;
const _ops: Record<number, Op> = {
  0x01: (a, b) => a + b,
  0x02: (a, b) => a - b,
  0x03: (a, b) => a * b,
  0x04: (a, b) => a / b,
  0x05: (a, b) => a % b,
  0x06: (a, b) => a ** b,
  0x07: (a)    => -a,
  0x08: (a, b) => (a > b ? a : b),
  0x09: (a, b) => (a < b ? a : b),
  0x0A: (a)    => Math.abs(a),
};

function _exec(opcode: number, ...args: number[]): number {
  return _ops[opcode](...args);
}

// original: (3 + 4) * (10 - 2) / 4
const r = _exec(0x04, _exec(0x03, _exec(0x01, 3, 4), _exec(0x02, 10, 2)), 4);
console.log(r);  // 14

console.log(_exec(0x06, 2, 10));    // 1024
console.log(_exec(0x07, -42));       // 42
console.log(_exec(0x08, 3, 7));     // 7
console.log(_exec(0x0A, -99));      // 99

// string opcode table (obfuscated method dispatch)
type StrOp = (s: string, ...args: any[]) => any;
const _sops: Record<string, StrOp> = {
  "\x73\x70": (s) => s.split(""),
  "\x6a\x6e": (s, sep) => s.split("").join(sep),
  "\x72\x76": (s) => s.split("").reverse().join(""),
  "\x75\x70": (s) => s.toUpperCase(),
  "\x6c\x6f": (s) => s.toLowerCase(),
};
console.log(_sops["\x72\x76"]("hello"));
console.log(_sops["\x75\x70"]("world"));
console.log(_sops["\x6a\x6e"]("abc", "-"));
