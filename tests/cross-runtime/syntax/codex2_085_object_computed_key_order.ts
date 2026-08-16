// Cross-runtime: computed object keys and values evaluate in source order.
const seen: string[] = [];
function key(n: string) { seen.push("k" + n); return n; }
function value(n: number) { seen.push("v" + n); return n; }
const o = { [key("a")]: value(1), fixed: value(2), [key("b")]: value(3) };
console.log(JSON.stringify(o));
console.log(seen.join(","));

