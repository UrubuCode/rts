// Cross-runtime: Object.hasOwn vs hasOwnProperty on prototype chain.
const proto = { inherited: 1 };
const obj = Object.create(proto);
obj.own = 2;
console.log("own=" + Object.hasOwn(obj, "own") + ":" + obj.hasOwnProperty("own"));
console.log("inh=" + Object.hasOwn(obj, "inherited") + ":" + obj.hasOwnProperty("inherited"));
console.log("miss=" + Object.hasOwn(obj, "zzz") + ":" + obj.hasOwnProperty("zzz"));
