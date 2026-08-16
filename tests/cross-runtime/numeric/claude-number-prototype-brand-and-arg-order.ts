// ONE thing: Number.prototype methods check the RECEIVER's [[NumberData]] slot
// before they touch the argument. A string receiver throws TypeError without
// ever calling the argument's valueOf — and Number.prototype is itself +0.

function show(label: string, fn: () => string): void {
  try {
    console.log(label + "=" + fn());
  } catch (e) {
    console.log(label + "!" + (e as any).constructor.name);
  }
}

const toFixed = Number.prototype.toFixed;
const toString = Number.prototype.toString;
const valueOf = Number.prototype.valueOf;
const toPrecision = Number.prototype.toPrecision;
const toExponential = Number.prototype.toExponential;

// --- a primitive number and a Number object both carry the slot ---
show("prim", () => toFixed.call(1.5, 1));
show("wrapper", () => toFixed.call(new Number(1.5), 1));
show("object_of", () => toFixed.call(Object(1.5), 1));
show("valueof_prim", () => String(valueOf.call(255)));
show("valueof_wrapper", () => String(valueOf.call(new Number(255))));
show("tostring_wrapper_16", () => toString.call(new Number(255), 16));

// --- everything else is refused, including a string that looks numeric ---
show("string", () => toFixed.call("1.5", 1));
show("string_wrapper", () => toFixed.call(new String("1.5"), 1));
show("boolean", () => toString.call(true));
show("null", () => String(valueOf.call(null)));
show("undefined", () => String(valueOf.call(undefined)));
show("plain_obj", () => String(valueOf.call({})));
show("obj_with_valueof", () => String(valueOf.call({ valueOf: () => 5 })));
show("array", () => toFixed.call([1.5], 1));
show("bigint", () => toString.call(1n));
show("symbol", () => toString.call(Symbol("s")));
show("date_proto", () => String(valueOf.call(Date.prototype)));

// --- Number.prototype IS a Number object holding +0 ---
show("proto_valueOf", () => String(valueOf.call(Number.prototype)));
show("proto_toFixed", () => Number.prototype.toFixed(2));
show("proto_String", () => String(Number.prototype));
console.log("proto_typeof=" + typeof Number.prototype);
console.log("proto_tag=" + Object.prototype.toString.call(Number.prototype));
console.log("wrapper_tag=" + Object.prototype.toString.call(new Number(1)));

// --- the brand check happens BEFORE the argument is coerced ---
const log: string[] = [];
const spy: any = {
  valueOf: function () {
    log.push("coerced");
    return 2;
  },
};
show("bad_receiver_with_spy", () => toFixed.call("nope", spy));
console.log("spy_log_after_bad_receiver=[" + log.join(",") + "]");
show("good_receiver_with_spy", () => toFixed.call(3.14159, spy));
console.log("spy_log_after_good_receiver=[" + log.join(",") + "]");

// --- and BEFORE the range check, so an out-of-range arg on a bad receiver
//     still reports the receiver problem ---
show("bad_receiver_bad_range", () => toFixed.call("nope", 500));
show("good_receiver_bad_range", () => toFixed.call(1, 500));
show("bad_receiver_exp", () => toExponential.call({}, 500));
show("bad_receiver_prec", () => toPrecision.call({}, 0));

// --- toString's radix is coerced only after the brand check passes ---
const radixLog: string[] = [];
const radixSpy: any = {
  valueOf: function () {
    radixLog.push("radix");
    return 2;
  },
};
show("tostring_radix_spy_bad", () => toString.call("5", radixSpy));
console.log("radix_log_after_bad=[" + radixLog.join(",") + "]");
show("tostring_radix_spy_good", () => toString.call(5, radixSpy));
console.log("radix_log_after_good=[" + radixLog.join(",") + "]");

// --- a subclass instance still carries the slot ---
class MyNumber extends Number {}
const sub = new MyNumber(7.25);
show("subclass_toFixed", () => toFixed.call(sub, 1));
show("subclass_valueOf", () => String(valueOf.call(sub)));
console.log("subclass_typeof=" + typeof sub);
console.log("subclass_tag=" + Object.prototype.toString.call(sub));

// --- an object that merely inherits from Number.prototype does not ---
const fake = Object.create(Number.prototype);
show("fake_valueOf", () => String(valueOf.call(fake)));
show("fake_toFixed", () => toFixed.call(fake, 2));
