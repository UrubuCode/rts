// Cross-runtime: an Array subclass propagates through map/filter/slice/splice
// via ArraySpeciesCreate, Symbol.species can send them back to plain Array, and
// length still tracks index writes on the subclass.
class MyArr<T> extends Array<T> {
  tag: string = "mine";
}

const m = new MyArr<number>();
m.push(1, 2, 3, 4);
console.log("len=" + m.length);
console.log("tag=" + m.tag);
console.log("is-array=" + Array.isArray(m));
console.log("tostring=" + Object.prototype.toString.call(m));
console.log("join=" + m.join(","));

const mapped: any = m.map((x) => x * 2);
console.log("map-ctor=" + mapped.constructor.name);
console.log("map-instanceof=" + (mapped instanceof MyArr));
console.log("map-tag=" + mapped.tag);
console.log("map-values=" + mapped.join(","));

const filtered: any = m.filter((x) => x % 2 === 0);
console.log("filter-instanceof=" + (filtered instanceof MyArr));
console.log("filter-values=" + filtered.join(","));

const sliced: any = m.slice(1, 3);
console.log("slice-instanceof=" + (sliced instanceof MyArr));
console.log("slice-values=" + sliced.join(","));

const spliced: any = m.slice().splice(0, 2);
console.log("splice-instanceof=" + (spliced instanceof MyArr));
console.log("splice-values=" + spliced.join(","));

// concat and flat also use species.
const conc: any = m.concat([9]);
console.log("concat-instanceof=" + (conc instanceof MyArr));
console.log("concat-values=" + conc.join(","));

// from/of construct `this`, so they answer the subclass; toSorted/toReversed
// use neither and always answer a plain Array.
const from: any = MyArr.from([1, 2, 3]);
console.log("from-instanceof=" + (from instanceof MyArr));
console.log("of-instanceof=" + (MyArr.of(1, 2) instanceof MyArr));
const tosorted: any = m.toSorted((a, b) => b - a);
console.log("tosorted-ctor=" + tosorted.constructor.name);
console.log("tosorted-values=" + tosorted.join(","));
const tored: any = m.toReversed();
console.log("toreversed-ctor=" + tored.constructor.name);

// Symbol.species sends the derived methods back to plain Array.
class PlainOut<T> extends Array<T> {
  static get [Symbol.species]() {
    return Array;
  }
}
const p = new PlainOut<number>();
p.push(1, 2, 3);
const pm: any = p.map((x) => x);
console.log("species-ctor=" + pm.constructor.name);
console.log("species-instanceof=" + (pm instanceof PlainOut));
console.log("species-is-array=" + Array.isArray(pm));
console.log("species-filter=" + (p.filter(() => true) instanceof PlainOut));

// length is still the exotic array length on the subclass.
const grow: any = new MyArr<number>();
grow[3] = "x";
console.log("grow-len=" + grow.length);
grow.length = 1;
console.log("grow-after-trunc=" + grow.length + ":" + String(grow[3]));

// new MyArr(n) takes the Array(n) length form.
const sized: any = new MyArr<number>(3);
console.log("sized-len=" + sized.length);
console.log("sized-holes=" + (0 in sized));
console.log("sized-instanceof=" + (sized instanceof MyArr));

// A subclass of a subclass keeps propagating.
class Deeper<T> extends MyArr<T> {}
const dd = new Deeper<number>();
dd.push(5, 6);
const dm: any = dd.map((x) => x);
console.log("deep-ctor=" + dm.constructor.name);
console.log("deep-instanceof-my=" + (dm instanceof MyArr));
console.log("deep-tag=" + dm.tag);

// Symbol.species on Array itself is a getter returning `this`.
const speciesDesc: any = Object.getOwnPropertyDescriptor(Array, Symbol.species);
console.log("array-species-getter=" + (typeof speciesDesc.get));
console.log("array-species-setter=" + String(speciesDesc.set));
console.log("array-species-value=" + (Array[Symbol.species] === Array));
console.log("myarr-species=" + (MyArr[Symbol.species] === MyArr));
