// Cross-runtime: type coercion edge cases and "wat" moments.

// + operator: string vs number
console.log(1 + "2");        // "12"
console.log("3" - 1);        // 2
console.log("3" * "4");      // 12
console.log("6" / "2");      // 3
console.log(true + true);    // 2
console.log(true + false);   // 1
console.log("" + false);     // "false"
console.log([] + []);        // ""
console.log({} + []);        // "[object Object]"
console.log([] + {});        // "[object Object]"

// == vs ===
console.log(0 == false);     // true
console.log("" == false);    // true
console.log(null == undefined); // true
console.log(null == 0);      // false
console.log(NaN == NaN);     // false
console.log(null === undefined); // false

// implicit boolean coercion
const falsy = [0, "", null, undefined, NaN, false];
console.log(falsy.filter(Boolean).length);  // 0
const truthy = [1, "a", [], {}, -1, Infinity];
console.log(truthy.filter(Boolean).length); // 6

// number coercions
console.log(+"");          // 0
console.log(+" ");         // 0
console.log(+"42");        // 42
console.log(+"0x10");      // 16
console.log(+true);        // 1
console.log(+false);       // 0
console.log(+null);        // 0
console.log(+undefined);   // NaN
console.log(+[]);          // 0
console.log(+[1]);         // 1
console.log(+[1,2]);       // NaN

// string coercions
console.log(String(null));       // "null"
console.log(String(undefined));  // "undefined"
console.log(String(true));       // "true"
console.log(String([]));         // ""
console.log(String([1,2,3]));    // "1,2,3"

// comparison coercions
console.log(null > 0);   // false
console.log(null == 0);  // false
console.log(null >= 0);  // true (!)
console.log("5" > 4);    // true (string→number)
console.log("5" > "40"); // true (lexicographic!)
