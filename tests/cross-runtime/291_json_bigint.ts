// Cross-runtime: JSON.stringify BigInt throws.
let out = "";
try { JSON.stringify({ x: 1n }); } catch (e: any) { out = e.name; }
console.log("err=" + out);
