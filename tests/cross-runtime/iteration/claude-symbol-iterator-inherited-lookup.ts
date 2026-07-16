// Cross-runtime: Symbol.iterator is looked up on the PROTOTYPE CHAIN, like any
// property. Focus: inheritance, shadowing, and dynamic add/remove.

// 1) Symbol.iterator on a parent prototype, used by a child instance
const parent: any = {
  [Symbol.iterator]() {
    let i = 0;
    return {
      next() {
        i++;
        return i <= 3 ? { value: "p" + i, done: false } : { value: undefined, done: true };
      }
    };
  }
};
const child = Object.create(parent);
console.log("inherited=" + [...(child as any)].join(","));

// 2) it is NOT an own property of the child
console.log("ownOnChild=" + Object.prototype.hasOwnProperty.call(child, Symbol.iterator));
console.log("ownOnParent=" + Object.prototype.hasOwnProperty.call(parent, Symbol.iterator));

// 3) an own Symbol.iterator SHADOWS the inherited one
const shadower = Object.create(parent);
shadower[Symbol.iterator] = function () {
  let i = 0;
  return {
    next() {
      i++;
      return i <= 2 ? { value: "own" + i, done: false } : { value: undefined, done: true };
    }
  };
};
console.log("shadowed=" + [...(shadower as any)].join(","));
console.log("parentUnaffected=" + [...(child as any)].join(","));

// 4) three levels deep
const grandchild = Object.create(Object.create(parent));
console.log("threeLevels=" + [...(grandchild as any)].join(","));

// 5) a class method named [Symbol.iterator] lives on the prototype
class Range {
  lo: number;
  hi: number;
  constructor(lo: number, hi: number) {
    this.lo = lo;
    this.hi = hi;
  }
  [Symbol.iterator]() {
    let cur = this.lo;
    const hi = this.hi;
    return {
      next() {
        return cur <= hi ? { value: cur++, done: false } : { value: undefined, done: true };
      }
    };
  }
}
const r = new Range(1, 4);
console.log("classRange=" + [...(r as any)].join(","));
console.log("onProtoNotInstance=" + Object.prototype.hasOwnProperty.call(r, Symbol.iterator) + "," + Object.prototype.hasOwnProperty.call(Range.prototype, Symbol.iterator));

// 6) a SUBCLASS inherits the iterator
class SubRange extends Range {}
console.log("subclass=" + [...(new SubRange(5, 7) as any)].join(","));

// 7) a subclass can OVERRIDE it
class RevRange extends Range {
  [Symbol.iterator]() {
    let cur = this.hi;
    const lo = this.lo;
    return {
      next() {
        return cur >= lo ? { value: cur--, done: false } : { value: undefined, done: true };
      }
    };
  }
}
console.log("override=" + [...(new RevRange(1, 4) as any)].join(","));

// 8) adding Symbol.iterator LATER makes an existing object iterable
const late: any = { a: 1 };
let lateWorked = "no";
try {
  [...late];
} catch (e) {
  lateWorked = "threwBefore";
}
late[Symbol.iterator] = function () {
  let done = false;
  return {
    next() {
      if (done) return { value: undefined, done: true };
      done = true;
      return { value: "late", done: false };
    }
  };
};
console.log("beforeAdd=" + lateWorked + "|afterAdd=" + [...late].join(","));

// 9) setting Symbol.iterator to undefined makes it non-iterable again
const removable: any = {
  [Symbol.iterator]() {
    return { next() { return { value: undefined, done: true }; } };
  }
};
let beforeRemove = "ok";
try {
  [...removable];
} catch (e) {
  beforeRemove = "threw";
}
removable[Symbol.iterator] = undefined;
let afterRemove = "ok";
try {
  [...removable];
} catch (e) {
  afterRemove = "threw";
}
console.log("removable=" + beforeRemove + "->" + afterRemove);

// 10) an array's iterator comes from Array.prototype, not the instance
const plainArr = [1, 2];
console.log("arrOwn=" + Object.prototype.hasOwnProperty.call(plainArr, Symbol.iterator));
console.log("arrProto=" + (plainArr[Symbol.iterator] === Array.prototype[Symbol.iterator]));

// 11) an object inheriting FROM an array is iterable through it
const arrChild: any = Object.create([7, 8, 9]);
console.log("arrChildIter=" + [...arrChild].join(","));
