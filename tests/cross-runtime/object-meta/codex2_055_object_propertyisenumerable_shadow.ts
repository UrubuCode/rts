// Cross-runtime: propertyIsEnumerable only inspects own descriptors.
const proto = { inherited: 1 };
const o = Object.create(proto);
Object.defineProperty(o, "hidden", { value: 2 });
o.visible = 3;
console.log(o.propertyIsEnumerable("inherited"));
console.log(o.propertyIsEnumerable("hidden"));
console.log(o.propertyIsEnumerable("visible"));

