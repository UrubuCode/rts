// Cross-runtime: stringify follows own-property ordering for integer-like keys.
const value: any = {};
value.z = 0;
value[10] = "ten";
value[2] = "two";
value.a = 1;
console.log(JSON.stringify(value));
console.log(Object.keys(value).join(","));

