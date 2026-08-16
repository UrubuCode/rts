// ONE thing: reduce's four-argument callback, its seed rule, and the TypeError
// it owes an empty array with no initial value.
const trace: string[] = [];
[10, 20, 30].reduce((acc, v, i, arr) => {
  trace.push("acc=" + acc + " v=" + v + " i=" + i + " len=" + arr.length + " same=" + (arr.length === 3));
  return acc + v;
});
console.log(trace.join(" | "));

const traceR: string[] = [];
[10, 20, 30].reduceRight((acc, v, i) => { traceR.push(acc + "/" + v + "@" + i); return acc + v; });
console.log("right=" + traceR.join(" "));

// With an initial value the callback runs once per element, including index 0.
let count = 0;
[1, 2, 3].reduce((a, b) => { count++; return a + b; }, 0);
console.log("withSeed=" + count);
count = 0;
[1, 2, 3].reduce((a, b) => { count++; return a + b; });
console.log("noSeed=" + count);

// A single element with no seed returns it WITHOUT calling the callback.
let called = false;
console.log("single=" + [7].reduce((a, b) => { called = true; return a + b; }) + " called=" + called);

// An empty array with no seed is a TypeError; with a seed it is the seed.
try { ([] as number[]).reduce((a, b) => a + b); } catch (e: any) { console.log("emptyNoSeed=" + e.constructor.name); }
console.log("emptySeed=" + ([] as number[]).reduce((a, b) => a + b, 42));

// An all-holes array behaves like an empty one.
const holes: any[] = [, , ];
try { holes.reduce((a: any, b: any) => a + b); } catch (e: any) { console.log("holesNoSeed=" + e.constructor.name); }
console.log("holesSeed=" + holes.reduce((a: any, b: any) => a + b, "s"));

// undefined as an explicit seed is a real seed, not an absent one.
console.log("undefSeed=" + String([1].reduce((a: any, b: any) => String(a) + "/" + b, undefined)));

// The seed rule with holes: the first PRESENT element becomes the accumulator.
const mixed: any[] = [, , 5, , 6];
console.log("firstPresent=" + mixed.reduce((a: any, b: any) => a + b));

// An element appended during the reduction is not visited.
const grow = [1, 2];
console.log("grow=" + grow.reduce((a, b, i) => { if (i === 0) grow.push(100); return a + b; }, 0) + " len=" + grow.length);

// A non-callable callback is a TypeError even for an empty array.
try { ([] as any).reduce(1, 0); } catch (e: any) { console.log("badCb=" + e.constructor.name); }

// reduce is generic over an array-like.
const like: any = { length: 3, 0: 1, 1: 2, 2: 3 };
console.log("generic=" + Array.prototype.reduce.call(like, (a: any, b: any) => a + b, 0));
