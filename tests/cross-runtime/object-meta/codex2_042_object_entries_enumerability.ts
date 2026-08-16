// Cross-runtime: Object.entries omits inherited and non-enumerable properties.
const proto = { inherited: 1 };
const o = Object.create(proto);
o.visible = 2;
Object.defineProperty(o, "hidden", { value: 3, enumerable: false });
console.log(JSON.stringify(Object.entries(o)));
console.log("inherited" in o, Object.hasOwn(o, "inherited"));

