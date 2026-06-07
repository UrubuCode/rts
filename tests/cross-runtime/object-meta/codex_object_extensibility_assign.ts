// Cross-runtime: Reflect behavior on non-extensible objects.
const obj: any = { a: 1 };
Object.preventExtensions(obj);
console.log("b" in obj);
console.log(Reflect.set(obj, "c", 3));
console.log(Object.isExtensible(obj));
console.log(Object.keys(obj).join(","));
