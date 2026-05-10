// Cross-runtime: JSON.parse with reviver
const json = '{"a":1,"b":2,"c":3}';
const obj = JSON.parse(json, (key, value) => {
  if (typeof value === "number") {
    return value * 2;
  }
  return value;
});

console.log("a=" + obj.a);
console.log("b=" + obj.b);
console.log("c=" + obj.c);

// Array
const arrJson = '[1,2,3]';
const arr = JSON.parse(arrJson, (key, value) => {
  if (typeof value === "number") {
    return value + 10;
  }
  return value;
});

console.log("arr=" + arr.join(","));
