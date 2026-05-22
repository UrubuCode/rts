// Cross-runtime: string array rotation pattern (js-confuser / javascript-obfuscator).
// All string literals moved to array; accessed by index via decode function.
const _0x3f2a = ["hello","world","foo","bar","baz","result","error","ok","done","start"];
function _0x1d(idx: number, shift: number) { return _0x3f2a[(idx + shift) % _0x3f2a.length]; }

console.log(_0x1d(0, 0));   // hello
console.log(_0x1d(1, 0));   // world
console.log(_0x1d(0, 1));   // world
console.log(_0x1d(9, 1));   // hello (wraps)

// string array with base64 decode (common obfuscator output)
const _str = ["aGVsbG8=","d29ybGQ=","Zm9v","YmFy"];
function _b64(s: string) { return atob(s); }
const _d = _str.map(_b64);
console.log(_d[0]);
console.log(_d[1]);
console.log(_d[2]);
console.log(_d[3]);

// indirect property access via computed string
const _k = ["l" + "e" + "n" + "g" + "t" + "h", "p" + "u" + "s" + "h", "j" + "o" + "i" + "n"];
const _arr: number[] = [];
(_arr as any)[_k[1]](1);
(_arr as any)[_k[1]](2);
(_arr as any)[_k[1]](3);
console.log((_arr as any)[_k[0]]);
console.log((_arr as any)[_k[2]](","));
