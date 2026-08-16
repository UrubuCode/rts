// ONE thing: Array.prototype methods are GENERIC — applied through call to a
// plain object with a length, they read and write it by index and update length.
const like: any = { length: 3, 0: "a", 1: "b", 2: "c" };

console.log("join=" + Array.prototype.join.call(like, "-"));
console.log("slice=" + JSON.stringify(Array.prototype.slice.call(like, 1)));
console.log("indexOf=" + Array.prototype.indexOf.call(like, "b"));
console.log("map=" + JSON.stringify(Array.prototype.map.call(like, (v: string) => v + "!")));
console.log("filter=" + JSON.stringify(Array.prototype.filter.call(like, (v: string) => v !== "b")));
console.log("includes=" + Array.prototype.includes.call(like, "c"));
console.log("at=" + Array.prototype.at.call(like, -1));

// push/pop mutate the object and its length.
const stack: any = { length: 0 };
Array.prototype.push.call(stack, "x", "y");
console.log("pushLen=" + stack.length + " v=" + stack[0] + stack[1]);
console.log("pop=" + Array.prototype.pop.call(stack) + " len=" + stack.length + " has1=" + (1 in stack));

// A length that is not a number is coerced by ToLength.
const weird: any = { length: "2", 0: 1, 1: 2, 2: 3 };
console.log("strLength=" + Array.prototype.join.call(weird, ","));

const negative: any = { length: -5, 0: 1 };
console.log("negLength=" + Array.prototype.join.call(negative, ",") + "|");

const fractional: any = { length: 2.7, 0: 1, 1: 2, 2: 3 };
console.log("fracLength=" + Array.prototype.join.call(fractional, ","));

// Missing indices behave as holes for the skipping methods.
const sparse: any = { length: 3, 1: "mid" };
let visits = 0;
Array.prototype.forEach.call(sparse, () => visits++);
console.log("sparseVisits=" + visits);
console.log("sparseJoin=" + Array.prototype.join.call(sparse, ","));

// Applied to a string, the read-only methods work and the writing ones do not.
console.log("strJoin=" + Array.prototype.join.call("abc", "."));
console.log("strMap=" + JSON.stringify(Array.prototype.map.call("ab", (c: string) => c.toUpperCase())));
console.log("strSlice=" + JSON.stringify(Array.prototype.slice.call("abc", 1)));

// Applied to a number or a boolean, length is undefined so nothing is read.
console.log("onNumber=" + JSON.stringify(Array.prototype.slice.call(5 as any)));
console.log("onBool=" + Array.prototype.join.call(true as any, ",") + "|");

// null and undefined are rejected before length is read.
try { Array.prototype.join.call(null as any, ","); } catch (e: any) { console.log("onNull=" + e.constructor.name); }
try { Array.prototype.map.call(undefined as any, (x: any) => x); } catch (e: any) { console.log("onUndef=" + e.constructor.name); }
