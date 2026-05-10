// Cross-runtime: object literals, nested access and computed keys.
const dynamic = "power";
const obj = {
  id: 7,
  ["user_" + 1]: "ana",
  [dynamic]: 9,
  nested: {
    value: 5,
  },
};

console.log("dynamic=" + obj[dynamic]);
console.log("computed=" + obj["user_" + 1]);
console.log("upper=" + obj["user_" + 1].toUpperCase());
console.log("chain=" + obj["user_" + 1] + ":" + obj.nested.value);
console.log("nested=" + `${obj.nested.value}:${obj["user_" + 1]}`);
