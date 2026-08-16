// Cross-runtime: repeated bind composes name and length metadata.
function original(a: any, b: any, c: any, d: any) {}
const once = original.bind(null, 1);
const twice = once.bind(null, 2, 3);
console.log(original.name, original.length);
console.log(once.name, once.length);
console.log(twice.name, twice.length);
console.log(Object.hasOwn(twice, "prototype"));

