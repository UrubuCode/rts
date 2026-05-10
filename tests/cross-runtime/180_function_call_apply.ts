// Cross-runtime: Function.call and apply
function greet(this: any, greeting: string, name: string) {
  return greeting + " " + name + (this.suffix || "");
}

const obj = { suffix: "!" };

console.log("call=" + greet.call(obj, "Hello", "World"));
console.log("apply=" + greet.apply(obj, ["Hi", "There"]));

// Without context
console.log("call_null=" + greet.call(null, "Hey", "You"));

// Math.max with apply
const nums = [1, 5, 3, 9, 2];
console.log("max=" + Math.max.apply(null, nums));
