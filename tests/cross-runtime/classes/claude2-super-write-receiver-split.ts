// Cross-runtime: `super.x = v` uses the home object's prototype to FIND the
// setter but the receiver (`this`) to store into — so a plain write lands as an
// own property of the instance and never touches the prototype, while a write
// that hits an inherited setter runs it with the derived instance as receiver.
const calls: string[] = [];

class Base {
  baseData: string = "base-data";

  get accessor(): string {
    calls.push("base-get");
    return "base-accessor";
  }
  set accessor(v: string) {
    calls.push("base-set:" + v);
    (this as any).stored = "stored-" + v;
  }
  get readOnly(): string {
    return "read-only";
  }
}
(Base.prototype as any).plainOnProto = "proto-value";

class Derived extends Base {
  stored: string = "";

  writePlain(v: string): string {
    super["plainOnProto"] = v;
    return "own=" + Object.prototype.hasOwnProperty.call(this, "plainOnProto")
      + ",proto=" + (Base.prototype as any).plainOnProto
      + ",read=" + (this as any).plainOnProto;
  }

  writeThroughSetter(v: string): string {
    super.accessor = v;
    return "stored=" + this.stored + ",own-accessor=" + Object.prototype.hasOwnProperty.call(this, "accessor");
  }

  writeReadOnly(v: string): string {
    try {
      super["readOnly"] = v;
      return "no-throw";
    } catch (e: any) {
      return e.constructor.name;
    }
  }

  readComputed(key: string): string {
    return "computed=" + String(super[key as any]);
  }

  viaArrow(): string {
    const f = () => super.accessor;
    return "arrow=" + f();
  }
}

const d = new Derived();

console.log("plain-write=" + d.writePlain("written"));
console.log("proto-after=" + (Base.prototype as any).plainOnProto);
console.log("instance-own=" + Object.prototype.hasOwnProperty.call(d, "plainOnProto"));
console.log("second-instance=" + String((new Derived() as any).plainOnProto));

console.log("setter-write=" + d.writeThroughSetter("v1"));
console.log("calls-after-setter=" + calls.join("|"));
console.log("stored-value=" + d.stored);

console.log("readonly-write=" + d.writeReadOnly("nope"));
console.log("readonly-value=" + (d as any).readOnly);
console.log("readonly-own=" + Object.prototype.hasOwnProperty.call(d, "readOnly"));

console.log("computed-accessor=" + d.readComputed("accessor"));
console.log("computed-plain=" + d.readComputed("plainOnProto"));
console.log("computed-missing=" + d.readComputed("nothing"));
console.log("calls-after-computed=" + calls.join("|"));

console.log("arrow=" + d.viaArrow());

// The getter's receiver is the derived instance, so an accessor that reads
// another property sees the derived state.
class Reader extends Base {
  own: string = "derived-own";
  get accessorPlus(): string {
    return super.accessor + "+" + this.own;
  }
}
const r = new Reader();
console.log("receiver=" + r.accessorPlus);

// Writing through super in a STATIC method targets the class object, and the
// inherited setter's `this` is the derived class.
class SBase {
  static get slot(): string {
    return "s-base";
  }
  static set slot(v: string) {
    calls.push("s-set:" + v);
    (this as any).kept = v;
  }
}
class SWriter extends SBase {
  static write(v: string): string {
    super.slot = v;
    return "kept=" + String((this as any).kept)
      + ",own=" + Object.prototype.hasOwnProperty.call(SWriter, "kept")
      + ",base-own=" + Object.prototype.hasOwnProperty.call(SBase, "kept");
  }
}
console.log("static-write=" + SWriter.write("sv"));
console.log("static-read=" + SWriter.slot);
console.log("calls-final=" + calls.join("|"));

// A frozen instance refuses the super-mediated write; class bodies are strict,
// so the assignment throws rather than failing silently.
class Frozen extends Base {
  attempt(): string {
    try {
      super["plainOnProto"] = "x";
      return "no-throw";
    } catch (e: any) {
      return e.constructor.name;
    }
  }
}
const f = new Frozen();
Object.freeze(f);
console.log("frozen-write=" + f.attempt());
console.log("frozen-own=" + Object.prototype.hasOwnProperty.call(f, "plainOnProto"));
console.log("frozen-read=" + (f as any).plainOnProto);

// Reflect.set spells out the same split explicitly: same target, different
// receiver, and the property lands on the receiver.
const target: any = Object.create(Base.prototype);
const receiver: any = {};
console.log("reflect-set=" + Reflect.set(target, "plainOnProto", "via-reflect", receiver));
console.log("reflect-target-own=" + Object.prototype.hasOwnProperty.call(target, "plainOnProto"));
console.log("reflect-receiver-own=" + Object.prototype.hasOwnProperty.call(receiver, "plainOnProto"));
console.log("reflect-receiver-value=" + receiver.plainOnProto);
console.log("reflect-setter=" + Reflect.set(target, "accessor", "rv", receiver));
console.log("reflect-setter-stored=" + String(receiver.stored));
console.log("calls-end=" + calls.join("|"));
