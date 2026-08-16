// Cross-runtime: addition coerces operands left-to-right before choosing concat or numeric mode.
const seen: string[] = [];
const left = { [Symbol.toPrimitive]() { seen.push("left"); return "L"; } };
const right = { [Symbol.toPrimitive]() { seen.push("right"); return 2; } };
console.log((left as any) + (right as any));
console.log(seen.join(","));

