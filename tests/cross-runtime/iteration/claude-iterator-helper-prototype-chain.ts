// Cross-runtime: where the iterator helpers LIVE. Every helper returns an
// object on one shared %IteratorHelperPrototype%, which sits on
// %Iterator.prototype%; the methods themselves are own properties of the
// latter, and their Symbol.toStringTag names are specified.

let n = 0;
function log(s: string): void { console.log((++n) + " " + s); }

function* g() { yield 1; yield 2; }

const helper: any = g().map(function (x: number) { return x; });
const helperProto = Object.getPrototypeOf(helper);
const iterProto = Object.getPrototypeOf(helperProto);

// 1) the tag on a helper object
log("1 tag=" + helper[Symbol.toStringTag]);
log("1 toString=" + Object.prototype.toString.call(helper));
log("1 tagIsOwnOfProto=" + Object.prototype.hasOwnProperty.call(helperProto, Symbol.toStringTag));
log("1 tagOwnOfHelper=" + Object.prototype.hasOwnProperty.call(helper, Symbol.toStringTag));

// 2) every lazy helper shares ONE prototype
const protos = ["map", "filter", "take", "drop", "flatMap"].map(function (k: string) {
  const args: any = k === "take" || k === "drop" ? 1 : function (x: any) { return x; };
  return Object.getPrototypeOf((g() as any)[k](args));
});
log("2 sharedProto=" + protos.every(function (p: any) { return p === helperProto; }));

// 3) that prototype owns next and return, and nothing else iterable-shaped
log("3 hasNext=" + Object.prototype.hasOwnProperty.call(helperProto, "next"));
log("3 hasReturn=" + Object.prototype.hasOwnProperty.call(helperProto, "return"));
log("3 hasThrow=" + Object.prototype.hasOwnProperty.call(helperProto, "throw"));
log("3 hasMap=" + Object.prototype.hasOwnProperty.call(helperProto, "map"));

// 4) one level up is Iterator.prototype, which owns Symbol.iterator and all
//    the helper methods
log("4 isIteratorPrototype=" + (iterProto === (Iterator as any).prototype));
log("4 ownsSymbolIterator=" + Object.prototype.hasOwnProperty.call(iterProto, Symbol.iterator));
log("4 parentIsObject=" + (Object.getPrototypeOf(iterProto) === Object.prototype));
log("4 constructorIsIterator=" + (iterProto.constructor === Iterator));

// 5) the helper method inventory, all own properties of Iterator.prototype
const names = ["map", "filter", "take", "drop", "flatMap", "reduce", "toArray", "forEach", "some", "every", "find"];
log("5 allOwn=" + names.every(function (k) { return Object.prototype.hasOwnProperty.call(iterProto, k); }));
log("5 allFunctions=" + names.every(function (k) { return typeof (iterProto as any)[k] === "function"; }));
log("5 arities=" + names.map(function (k) { return k + ":" + (iterProto as any)[k].length; }).join(","));
log("5 nonEnumerable=" + names.every(function (k) {
  return Object.getOwnPropertyDescriptor(iterProto, k).enumerable === false;
}));

// 6) a generator object reaches the helpers through the same chain
const genObj: any = g();
const genProto = Object.getPrototypeOf(Object.getPrototypeOf(genObj));
log("6 generatorReachesIteratorProto=" + (Object.getPrototypeOf(genProto) === iterProto));
log("6 generatorHasMap=" + (typeof genObj.map));

// 7) so do the built-in iterators
log("7 arrayIterator=" + (typeof [1][Symbol.iterator]().map));
log("7 stringIterator=" + (typeof "a"[Symbol.iterator]().map));
log("7 mapIterator=" + (typeof new Map()[Symbol.iterator]().map));
log("7 setIterator=" + (typeof new Set()[Symbol.iterator]().map));
log("7 regexpIterator=" + (typeof "aa".matchAll(/a/g).map));

// 8) Symbol.iterator on Iterator.prototype answers `this`
const someIt: any = [1][Symbol.iterator]();
log("8 returnsThis=" + (iterProto[Symbol.iterator].call(someIt) === someIt));
log("8 helperReturnsThis=" + (helper[Symbol.iterator]() === helper));

// 9) the tag on the built-in iterator prototypes
log("9 arrayIteratorTag=" + Object.prototype.toString.call([1][Symbol.iterator]()));
log("9 stringIteratorTag=" + Object.prototype.toString.call("a"[Symbol.iterator]()));
log("9 mapIteratorTag=" + Object.prototype.toString.call(new Map()[Symbol.iterator]()));
log("9 setIteratorTag=" + Object.prototype.toString.call(new Set()[Symbol.iterator]()));
log("9 generatorTag=" + Object.prototype.toString.call(genObj));

// 10) the helper still works after all that poking
log("10 values=" + helper.toArray().join(","));

console.log("end");
