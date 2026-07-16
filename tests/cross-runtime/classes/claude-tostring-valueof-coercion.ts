// Cross-runtime: toString/valueOf defined on a class drive coercion.
class Money {
  amount: number;
  constructor(amount: number) {
    this.amount = amount;
  }
  toString(): string {
    return "$" + this.amount;
  }
  valueOf(): number {
    return this.amount;
  }
}

const m = new Money(5);
// numeric context -> valueOf
console.log("unary_plus=" + +m);
console.log("mul=" + m * 2);
console.log("sub=" + (m - 1));
console.log("cmp=" + (m > 3));
console.log("number=" + Number(m));

// string context -> toString
console.log("template=" + `${m}`);
console.log("string=" + String(m));
console.log("concat_str=" + ("v:" + String(m)));

// `+` with a valueOf present prefers the number (hint "default")
console.log("plus_str=" + (m + ""));
console.log("plus_num=" + (m + 1));

// toString only: numeric coercion falls back to parsing the string
class Tag {
  v: string;
  constructor(v: string) {
    this.v = v;
  }
  toString(): string {
    return this.v;
  }
}
const t = new Tag("42");
console.log("tag_str=" + `${t}`);
console.log("tag_num=" + +t);
console.log("tag_plus=" + (t + 1));

const bad = new Tag("abc");
console.log("bad_num=" + +bad);
console.log("bad_plus=" + (bad + 1));

// valueOf only: string coercion uses the number
class Raw {
  valueOf(): number {
    return 9;
  }
}
const raw = new Raw();
console.log("raw_num=" + +raw);
console.log("raw_str=" + String(raw));
console.log("raw_template=" + `${raw}`);

// inherited toString/valueOf
class Cents extends Money {
  toString(): string {
    return super.toString() + "c";
  }
}
const c = new Cents(3);
console.log("cents_str=" + `${c}`);
console.log("cents_num=" + +c);
console.log("cents_plus=" + (c + 1));

// coercion inside array join (uses toString)
console.log("join=" + [new Money(1), new Money(2)].join("|"));

// sorting by valueOf-driven comparison
const sorted = [new Money(3), new Money(1), new Money(2)]
  .sort((a, b) => a.valueOf() - b.valueOf())
  .map((x) => x.toString());
console.log("sorted=" + sorted.join(","));
