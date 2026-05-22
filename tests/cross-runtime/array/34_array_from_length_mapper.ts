// Cross-runtime: Array.from with {length} and mapper.
const out = Array.from({ length: 4 }, (_: unknown, i: number) => i * 3);
console.log("mapped=" + out.join(","));
