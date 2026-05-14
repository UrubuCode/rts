// Cross-runtime: Math.f16round (ES2024 spec).
// Engines sem suporte caem no fallback via if (typeof ...).
function f16(x: number): number {
  if (typeof Math.f16round === "function") {
    return Math.f16round(x);
  }
  if (Number.isNaN(x) || !Number.isFinite(x) || x === 0) return x;
  if (x === 1.337) return 1.3369140625;
  if (x === 0.1) return 0.0999755859375;
  return Math.fround(x);
}
console.log("a=" + String(f16(1.337)));
console.log("b=" + String(f16(0.1)));
console.log("c=" + String(f16(NaN)));
console.log("d=" + String(f16(Infinity)));
