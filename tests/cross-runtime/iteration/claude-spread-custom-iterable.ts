// Cross-runtime: spread (...) consumes a custom iterable via the iteration
// protocol. Focus: spread ALWAYS drains to done, in every spread position.

function makeIterable(vals: any[], tag: string, log: string[]) {
  return {
    [Symbol.iterator]() {
      let i = 0;
      return {
        next() {
          log.push(tag + ":next" + i);
          if (i < vals.length) return { value: vals[i++], done: false };
          return { value: undefined, done: true };
        }
      };
    }
  };
}

// 1) spread into an array literal
const log1: string[] = [];
const it1 = makeIterable([1, 2, 3], "a", log1);
const arr1 = [...(it1 as any)];
console.log("arrayLiteral=" + arr1.join(","));
console.log("arrayLiteralCalls=" + log1.length);

// 2) spread in the MIDDLE of an array literal
const log2: string[] = [];
const arr2 = [0, ...(makeIterable([1, 2], "b", log2) as any), 9];
console.log("middle=" + arr2.join(","));

// 3) two spreads in one literal
const log3: string[] = [];
const arr3 = [
  ...(makeIterable(["x"], "c", log3) as any),
  ...(makeIterable(["y", "z"], "d", log3) as any)
];
console.log("twoSpreads=" + arr3.join(","));

// 4) spread into a function call's args
const log4: string[] = [];
function sum3(a: number, b: number, c: number) {
  return a + b + c;
}
console.log("callSpread=" + sum3(...(makeIterable([1, 2, 3], "e", log4) as any)));

// 5) spread of an EMPTY custom iterable
const log5: string[] = [];
const arr5 = [...(makeIterable([], "f", log5) as any)];
console.log("emptySpread=" + arr5.length + "|calls=" + log5.length);

// 6) spread drains fully: exactly N+1 next() calls for N values
const log6: string[] = [];
const arr6 = [...(makeIterable([1, 2, 3, 4, 5], "g", log6) as any)];
console.log("drainLen=" + arr6.length + "|nextCalls=" + log6.length);

// 7) spread of an iterable yielding objects keeps identity
const obj = { k: 1 };
const log7: string[] = [];
const arr7 = [...(makeIterable([obj, obj], "h", log7) as any)];
console.log("identity=" + (arr7[0] === obj) + "," + (arr7[0] === arr7[1]));

// 8) spread into new Set / new Map-ish consumer keeps order
const log8: string[] = [];
const arr8 = [...(makeIterable([3, 1, 3, 2], "i", log8) as any)];
console.log("orderPreserved=" + arr8.join(","));

// 9) undefined/null values from the iterator survive the spread
const log9: string[] = [];
const arr9 = [...(makeIterable([undefined, null, 0], "j", log9) as any)];
console.log("holesLen=" + arr9.length);
console.log("holeVals=" + String(arr9[0]) + "," + String(arr9[1]) + "," + String(arr9[2]));
