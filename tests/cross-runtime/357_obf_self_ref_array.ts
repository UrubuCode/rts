// Cross-runtime: self-referencing decode array with rotation (javascript-obfuscator style).
// Array initialized first, then rotated N times; index lookups work after rotation.

const _tbl: string[] = (function() {
  const arr = ["alpha","beta","gamma","delta","epsilon","zeta","eta","theta"];
  // rotate by 2: shift front and push to back, twice
  for (let i = 0; i < 2; i++) arr.push(arr.shift()!);
  return arr;
})();
// After 2 rotations: ["gamma","delta","epsilon","zeta","eta","theta","alpha","beta"]
console.log(_tbl[0]);  // gamma
console.log(_tbl[6]);  // alpha
console.log(_tbl[7]);  // beta

// lookup helper simulating obfuscated string access
function _s(idx: number): string { return _tbl[idx]; }
console.log(_s(0) + " " + _s(6));  // gamma alpha
console.log(_s(1));                  // delta

// index arithmetic to obscure which string is used
const _magic = 0x3;
console.log(_tbl[_magic]);           // zeta (index 3)
console.log(_tbl[_magic + 1]);       // eta
console.log(_tbl[(_magic * 2) - 1]); // theta (5)

// chained rotations with decode function (obfuscator decode table pattern)
const _encoded = (function() {
  const src = ["10","20","30","40","50","60","70","80"];
  // rotate by (src.length - 3) = 5
  const rot = src.length - 3;
  for (let i = 0; i < rot; i++) src.push(src.shift()!);
  return src;
})();
// After 5 rotations of 8: ["60","70","80","10","20","30","40","50"]
function _dec(idx: number): number { return parseInt(_encoded[idx]); }
console.log(_dec(0));  // 60
console.log(_dec(3));  // 10
console.log(_dec(7));  // 50
