// Cross-runtime: Math.clz32 and Math.imul
console.log("clz32_1=" + Math.clz32(1));
console.log("clz32_2=" + Math.clz32(2));
console.log("clz32_4=" + Math.clz32(4));
console.log("clz32_0=" + Math.clz32(0));
console.log("clz32_-1=" + Math.clz32(-1));

console.log("imul_2_3=" + Math.imul(2, 3));
console.log("imul_-1_8=" + Math.imul(-1, 8));
console.log("imul_0xffffffff_5=" + Math.imul(0xffffffff, 5));
console.log("imul_0xfffffffe_5=" + Math.imul(0xfffffffe, 5));
