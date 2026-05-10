// Cross-runtime: conservative function/IIFE coverage.
const base = 4;
const add = (value: number): number => base + value;
const iife = (function (x: number): number {
  return x * 2;
})(6);

console.log("add=" + add(5));
console.log("iife=" + iife);
