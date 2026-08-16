// Cross-runtime: statics are INHERITED through the constructor chain, not
// copied. A subclass reads the base's static until it writes one of its own,
// and `this` inside a static method is whichever constructor the call went
// through.
class Base {
  static count: number = 0;
  static label: string = "base";
  static tally: string[] = [];

  static bump(): string {
    (this as any).count = (this as any).count + 1;
    return (this as any).label + ":" + (this as any).count;
  }
  static whoAmI(): string {
    return (this as any).name;
  }
  static get computed(): string {
    return "computed-of-" + (this as any).label;
  }
}

class Sub extends Base {}
class SubSub extends Sub {
  static label: string = "subsub";
}

// Nothing is copied: the property lives only on Base until someone writes.
console.log("base-own-count=" + Object.prototype.hasOwnProperty.call(Base, "count"));
console.log("sub-own-count=" + Object.prototype.hasOwnProperty.call(Sub, "count"));
console.log("sub-reads=" + Sub.count + "," + Sub.label);
console.log("subsub-own-label=" + Object.prototype.hasOwnProperty.call(SubSub, "label"));
console.log("subsub-reads=" + SubSub.count + "," + SubSub.label);

// A static METHOD called through the subclass sees `this === Sub`, so the
// write it performs lands on Sub, leaving Base untouched.
console.log("bump-base=" + Base.bump());
console.log("bump-sub=" + Sub.bump());
console.log("bump-sub2=" + Sub.bump());
console.log("after-base-count=" + Base.count);
console.log("after-sub-count=" + Sub.count);
console.log("sub-own-count-now=" + Object.prototype.hasOwnProperty.call(Sub, "count"));
console.log("subsub-count=" + SubSub.count);
console.log("bump-subsub=" + SubSub.bump());
console.log("after-subsub=" + SubSub.count + "," + Sub.count + "," + Base.count);

// `this.name` inside a static method is the constructor it was reached from.
console.log("who-base=" + Base.whoAmI());
console.log("who-sub=" + Sub.whoAmI());
console.log("who-subsub=" + SubSub.whoAmI());
console.log("who-detached=" + (Base.whoAmI as any).call({ name: "borrowed" }));

// A static ACCESSOR is looked up the same chain and also sees the receiver.
console.log("computed-base=" + Base.computed);
console.log("computed-sub=" + Sub.computed);
console.log("computed-subsub=" + SubSub.computed);

// A MUTABLE object held in a base static is genuinely shared until shadowed.
Base.tally.push("from-base");
Sub.tally.push("from-sub");
console.log("shared-tally=" + Base.tally.join(","));
console.log("same-array=" + (Base.tally === Sub.tally));
(SubSub as any).tally = ["own"];
(SubSub as any).tally.push("more");
console.log("subsub-tally=" + (SubSub as any).tally.join(","));
console.log("base-tally-unchanged=" + Base.tally.join(","));

// Deleting the shadow makes the inherited one visible again.
console.log("delete-shadow=" + Reflect.deleteProperty(SubSub, "label"));
console.log("subsub-label-after=" + SubSub.label);
console.log("subsub-own-label-after=" + Object.prototype.hasOwnProperty.call(SubSub, "label"));

// A static field initialiser runs with `this` bound to its OWN class, and can
// read an inherited static from the chain.
class Derived extends Base {
  static seed: string = (this as any).label + "-seed";
  static self: boolean = (this as any) === Derived;
  static viaSuper: string = "n/a";
  static {
    (this as any).viaSuper = "block:" + (this as any).seed + ":" + (this as any).name;
  }
}
console.log("derived-seed=" + Derived.seed);
console.log("derived-self=" + Derived.self);
console.log("derived-block=" + Derived.viaSuper);
console.log("derived-own=" + Object.getOwnPropertyNames(Derived).sort().join(","));

// `super` on the static side reaches the base constructor's own statics.
class WithSuper extends Base {
  static label: string = "with-super";
  static both(): string {
    return (this as any).label + "/" + (super.whoAmI as any).call(Base) + "/" + super.computed;
  }
}
console.log("with-super=" + WithSuper.both());
console.log("with-super-computed=" + WithSuper.computed);
