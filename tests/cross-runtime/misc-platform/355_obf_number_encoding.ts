// Cross-runtime: number literal obfuscation (js-confuser encodes numbers as expressions).
// Obfuscators replace 42 with (~(~42)), 0 with (1-1), 1 with (!!1|0), etc.

// bitwise double NOT (common obfuscator trick — truncates to int32)
console.log(~~42);       // 42
console.log(~~3.9);      // 3 (truncates)
console.log(~~(-3.9));   // -3
console.log(~~0);        // 0

// arithmetic encoding of constants
const _0 = (1 - 1);
const _1 = (!!1 | 0);
const _2 = (_1 + _1);
const _10 = (_1 + _1 + _1 + _1 + _1 + _1 + _1 + _1 + _1 + _1);
console.log(_0, _1, _2, _10);

// XOR identity: n^0 = n, n^n = 0
console.log(42 ^ 0);
console.log(42 ^ 42);
console.log(0 ^ 99);

// shift-based multiply/divide
console.log(1 << 8);    // 256
console.log(1024 >> 2); // 256
console.log(7 << 3);    // 56
console.log(100 >>> 1); // 50

// bitmask extraction (obfuscated field packing)
const packed = 0xDEADBEEF;
const byte0 = packed & 0xFF;
const byte1 = (packed >> 8) & 0xFF;
const byte2 = (packed >> 16) & 0xFF;
const byte3 = (packed >> 24) & 0xFF;
console.log(byte0.toString(16));   // ef
console.log(byte1.toString(16));   // be
console.log(byte2.toString(16));   // ad
console.log(byte3.toString(16));   // de (signed)... but >>> to unsigned:
console.log(((packed >>> 24) & 0xFF).toString(16));  // de
