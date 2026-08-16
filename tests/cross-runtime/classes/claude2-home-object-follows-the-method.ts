// Cross-runtime: `super` resolves against the method's [[HomeObject]] — the
// object it was DEFINED in — not against the receiver. Copy a method to
// another class and its super still points back where it was written; assign a
// plain function instead and there is no super to resolve at all.
class Base {
  who(): string {
    return "Base.who";
  }
  tag(): string {
    return "base-tag";
  }
}

class Middle extends Base {
  who(): string {
    return "Middle(" + super.who() + ")";
  }
  tag(): string {
    return "middle-tag";
  }
}

class Other extends Base {
  who(): string {
    return "Other.who";
  }
  tag(): string {
    return "other-tag";
  }
}

const m = new Middle();
console.log("middle-direct=" + m.who());

// Borrow Middle's method onto an Other instance: `this` becomes the Other
// instance, but super.who() still reaches Base.prototype.who.
const borrowed = Middle.prototype.who;
const o = new Other();
console.log("borrowed-on-other=" + borrowed.call(o));
console.log("borrowed-this-tag=" + (o as any).tag());

// Copy the method onto an unrelated class's prototype. The home object was
// fixed at definition time, so super still means Base.prototype.
class Unrelated {
  tag(): string {
    return "unrelated-tag";
  }
}
(Unrelated.prototype as any).who = Middle.prototype.who;
const u: any = new Unrelated();
console.log("copied-who=" + u.who());
console.log("copied-instanceof-base=" + (u instanceof Base));
console.log("copied-tag=" + u.tag());

// Moving Middle.prototype under a DIFFERENT parent changes where super goes,
// because the home object is the prototype object itself and super walks its
// current [[Prototype]].
class Replacement {
  who(): string {
    return "Replacement.who";
  }
}
const savedParent = Object.getPrototypeOf(Middle.prototype);
Object.setPrototypeOf(Middle.prototype, Replacement.prototype);
console.log("after-reparent=" + m.who());
Object.setPrototypeOf(Middle.prototype, savedParent);
console.log("restored=" + m.who());

// An object literal's method has a home object too, and setPrototypeOf on the
// literal retargets its super.
const literal: any = {
  who(): string {
    return "literal(" + super.who() + ")";
  },
};
Object.setPrototypeOf(literal, Base.prototype);
console.log("literal-base=" + literal.who());
Object.setPrototypeOf(literal, Other.prototype);
console.log("literal-other=" + literal.who());

// A method shorthand and a function-valued property differ: only the shorthand
// gets a home object, so only it may write `super`. The function property is
// also enumerable and constructible.
const shorthandDesc: any = Object.getOwnPropertyDescriptor(
  { m(): number { return 1; } },
  "m",
);
const propDesc: any = Object.getOwnPropertyDescriptor(
  { m: function (): number { return 1; } },
  "m",
);
console.log("shorthand-enumerable=" + shorthandDesc.enumerable);
console.log("prop-enumerable=" + propDesc.enumerable);
console.log("shorthand-has-prototype=" + Object.prototype.hasOwnProperty.call(shorthandDesc.value, "prototype"));
console.log("prop-has-prototype=" + Object.prototype.hasOwnProperty.call(propDesc.value, "prototype"));
console.log("shorthand-name=" + shorthandDesc.value.name);
console.log("prop-name=" + propDesc.value.name);

let newShorthand = "no-throw";
try {
  new (shorthandDesc.value as any)();
} catch (e: any) {
  newShorthand = e.constructor.name;
}
console.log("new-shorthand=" + newShorthand);
console.log("new-prop=" + typeof new (propDesc.value as any)());

// A class METHOD is likewise non-enumerable and non-constructible, while a
// class FIELD holding a function is enumerable and constructible.
class Mixed {
  asMethod(): number {
    return 1;
  }
  asField: () => number = function (): number {
    return 2;
  };
}
const mi: any = new Mixed();
console.log("class-method-own=" + Object.prototype.hasOwnProperty.call(mi, "asMethod"));
console.log("class-field-own=" + Object.prototype.hasOwnProperty.call(mi, "asField"));
console.log("class-keys=" + Object.keys(mi).join(","));
console.log("field-has-prototype=" + Object.prototype.hasOwnProperty.call(mi.asField, "prototype"));
console.log("field-name=" + JSON.stringify(mi.asField.name));
console.log("method-name=" + Mixed.prototype.asMethod.name);

// Static methods have the constructor as their home object, so static super
// reaches the parent CONSTRUCTOR, not the parent prototype.
class SBase {
  static shout(): string {
    return "SBase.shout";
  }
}
class SDerived extends SBase {
  static shout(): string {
    return "SDerived(" + super.shout() + ")";
  }
}
console.log("static-super=" + SDerived.shout());
console.log("static-super-borrowed=" + (SDerived.shout as any).call(SBase));
