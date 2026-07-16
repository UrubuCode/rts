// Cross-runtime: array destructuring pulls ONLY as many values as it binds,
// then closes the iterator. Focus: laziness + IteratorClose on partial drain.

function tracked(log: string[]) {
  let i = 0;
  return {
    [Symbol.iterator]() {
      return {
        next() {
          i++;
          log.push("next" + i);
          return { value: i, done: false }; // INFINITE: never done
        },
        return() {
          log.push("return");
          return { done: true, value: undefined };
        }
      };
    }
  };
}

// 1) two bindings off an infinite iterator => exactly 2 next() + 1 return()
const l1: string[] = [];
const [a1, b1] = tracked(l1) as any;
console.log("two=" + a1 + "," + b1);
console.log("twoLog=" + l1.join(","));

// 2) a single binding pulls exactly once
const l2: string[] = [];
const [a2] = tracked(l2) as any;
console.log("one=" + a2 + "|log=" + l2.join(","));

// 3) zero bindings: [] still gets the iterator and closes it, pulls nothing
const l3: string[] = [];
const [] = tracked(l3) as any;
console.log("zeroLog=" + l3.join(",") + "|len=" + l3.length);

// 4) a hole skips a slot but STILL pulls that value
const l4: string[] = [];
const [, b4] = tracked(l4) as any;
console.log("hole=" + b4 + "|log=" + l4.join(","));

// 5) rest DRAINS: no return() because the iterator hit done itself
const l5: string[] = [];
const finite = {
  [Symbol.iterator]() {
    let i = 0;
    return {
      next() {
        i++;
        l5.push("next" + i);
        return i <= 3 ? { value: i, done: false } : { value: undefined, done: true };
      },
      return() {
        l5.push("return");
        return { done: true, value: undefined };
      }
    };
  }
};
const [f1, ...restF] = finite as any;
console.log("rest=" + f1 + "|" + restF.join(",") + "|log=" + l5.join(","));

// 6) exhausted-exactly: N bindings over N values => no return() (done reached)
const l6: string[] = [];
const two = {
  [Symbol.iterator]() {
    let i = 0;
    return {
      next() {
        i++;
        l6.push("next" + i);
        return i <= 2 ? { value: i, done: false } : { value: undefined, done: true };
      },
      return() {
        l6.push("return");
        return { done: true, value: undefined };
      }
    };
  }
};
const [x6, y6, z6] = two as any;
console.log("over=" + x6 + "," + y6 + "," + String(z6));
console.log("overLog=" + l6.join(","));

// 7) defaults only apply to undefined coming out of the iterator
const l7: string[] = [];
const withUndef = {
  [Symbol.iterator]() {
    let i = 0;
    const vals = [undefined, null, 5];
    return {
      next() {
        l7.push("next" + i);
        if (i < vals.length) return { value: vals[i++], done: false };
        return { value: undefined, done: true };
      }
    };
  }
};
const [d1 = "DEF", d2 = "DEF", d3 = "DEF"] = withUndef as any;
console.log("defaults=" + String(d1) + "," + String(d2) + "," + String(d3));

// 8) swap via destructuring an array (uses the array iterator)
let s1 = 1;
let s2 = 2;
[s1, s2] = [s2, s1];
console.log("swap=" + s1 + "," + s2);

// 9) nested destructuring pulls lazily at each level
const l9: string[] = [];
const nested = {
  [Symbol.iterator]() {
    let i = 0;
    return {
      next() {
        i++;
        l9.push("outer" + i);
        return { value: [i, i * 100], done: false };
      },
      return() {
        l9.push("outerReturn");
        return { done: true, value: undefined };
      }
    };
  }
};
const [[n1, n2]] = nested as any;
console.log("nested=" + n1 + "," + n2 + "|log=" + l9.join(","));
