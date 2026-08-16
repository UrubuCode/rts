// Cross-runtime: `Iterator.from` adopts something that merely has `next` and
// gives it the helper methods. Focus: when it returns the argument UNCHANGED,
// when it wraps, and how the wrapper forwards next/return.

let n = 0;
function log(s: string): void { console.log((++n) + " " + s); }

const trace: string[] = [];

// 1) a plain object with only `next` -- no Symbol.iterator anywhere
const bare: any = {
  i: 0,
  next: function () {
    trace.push("next" + this.i);
    this.i++;
    return this.i <= 3 ? { value: this.i * 100, done: false } : { value: undefined, done: true };
  }
};
log("1 hasHelpersBefore=" + (typeof bare.map));
const wrapped: any = Iterator.from(bare);
log("1 hasHelpersAfter=" + (typeof wrapped.map));
log("1 wrapperIsNotSource=" + (wrapped !== bare));
log("1 values=" + wrapped.map(function (x: number) { return x + 1; }).toArray().join(","));
log("1 trace=" + trace.join(","));

// 2) the wrapper is its own iterable
trace.length = 0;
bare.i = 0;
const w2: any = Iterator.from(bare);
log("2 selfIterable=" + (w2[Symbol.iterator]() === w2));
log("2 spread=" + [...w2].join(","));

// 3) an object that is ITERABLE is iterated through Symbol.iterator
trace.length = 0;
const iterable: any = {
  [Symbol.iterator]: function () {
    trace.push("symbolIteratorCalled");
    let k = 0;
    return { next: function () { k++; return k <= 2 ? { value: "v" + k, done: false } : { value: undefined, done: true }; } };
  }
};
const w3: any = Iterator.from(iterable);
log("3 trace=" + trace.join(","));
log("3 values=" + w3.toArray().join(","));

// 4) a built-in iterator ALREADY inherits Iterator.prototype, so it comes back
//    unchanged rather than wrapped
const arrIt: any = [1, 2][Symbol.iterator]();
log("4 identity=" + (Iterator.from(arrIt) === arrIt));
const strIt: any = "ab"[Symbol.iterator]();
log("4 stringIdentity=" + (Iterator.from(strIt) === strIt));
const mapIt: any = new Map([["k", 1]])[Symbol.iterator]();
log("4 mapIdentity=" + (Iterator.from(mapIt) === mapIt));
function* gen() { yield 1; }
const genIt: any = gen();
log("4 generatorIdentity=" + (Iterator.from(genIt) === genIt));

// 5) an array is iterable, so Iterator.from wraps its ITERATOR, not the array
const w5: any = Iterator.from([5, 6, 7]);
log("5 isArray=" + Array.isArray(w5) + " values=" + w5.take(2).toArray().join(","));

// 6) the wrapper forwards return() to the source when the source has one
trace.length = 0;
const closable: any = {
  i: 0,
  next: function () { this.i++; trace.push("next" + this.i); return { value: this.i, done: false }; },
  return: function (v: any) { trace.push("return:" + String(v)); return { value: v, done: true }; }
};
const w6: any = Iterator.from(closable);
log("6 first=" + w6.next().value);
log("6 returned=" + JSON.stringify(w6.return("bye")));
log("6 trace=" + trace.join(","));

// 7) a source with NO return() is closed without error
trace.length = 0;
const noReturn: any = { i: 0, next: function () { this.i++; trace.push("n" + this.i); return { value: this.i, done: false }; } };
const w7: any = Iterator.from(noReturn);
w7.next();
log("7 returned=" + JSON.stringify(w7.return()));
log("7 trace=" + trace.join(","));

// 8) take() over a wrapped source closes it through the wrapper
trace.length = 0;
closable.i = 0;
log("8 taken=" + Iterator.from(closable).take(2).toArray().join(","));
log("8 trace=" + trace.join(","));

// 9) bad arguments
log("9 fromNumber=" + (function () { try { Iterator.from(42 as any); return "no"; } catch (e: any) { return e.constructor.name; } })());
log("9 fromNull=" + (function () { try { Iterator.from(null as any); return "no"; } catch (e: any) { return e.constructor.name; } })());
// an object with no `next` is accepted at from() time and only fails on use
log("9 fromNoNextBuilds=" + (function () { try { Iterator.from({} as any); return "ok"; } catch (e: any) { return e.constructor.name; } })());
log("9 fromNoNextPulls=" + (function () { try { Iterator.from({} as any).next(); return "no"; } catch (e: any) { return e.constructor.name; } })());
log("9 fromString=" + Iterator.from("abc" as any).toArray().join("-"));

// 10) Iterator itself is not constructible, but is a function
log("10 typeofIterator=" + typeof Iterator);
log("10 newIterator=" + (function () { try { new (Iterator as any)(); return "no"; } catch (e: any) { return e.constructor.name; } })());
log("10 fromLength=" + (Iterator as any).from.length);
log("10 fromName=" + (Iterator as any).from.name);

console.log("end");
